//! どこで: wrapperのRPC補助層 / 何を: eth系参照ロジックを分離 / なぜ: canister entrypointの責務を薄くするため

use evm_core::{chain, hash};
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::chain_data::{
    BlockData, ReceiptLike, StoredTx, StoredTxBytes, TxId, TxKind, TxLoc, TxLocKind,
};
use evm_db::stable_state::with_state;
use evm_db::types::keys::{make_account_key, make_code_key, make_storage_key};
use ic_evm_rpc_types::{
    DecodedTxView, EthBlockView, EthLogFilterView, EthLogItemView, EthLogsCursorView,
    EthLogsPageView, EthReceiptLogView, EthReceiptView, EthTxListView, EthTxView, GetLogsErrorView,
    RpcAccessListItemView, RpcBlockLookupView, RpcBlockTagView, RpcCallObjectView,
    RpcCallResultView, RpcErrorView, RpcFeeHistoryView, RpcHistoryWindowView, RpcReceiptLookupView,
    SubmitTxError, TxKindView,
};
use tracing::{error, warn};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TxApiErrorKind {
    InvalidArgument,
    Rejected,
}

const CODE_ARG_TX_TOO_LARGE: &str = "arg.tx_too_large";
const CODE_ARG_DECODE_FAILED: &str = "arg.decode_failed";
const CODE_ARG_DERIVATION_FAILED: &str = "arg.principal_to_evm_derivation_failed";
const CODE_ARG_UNSUPPORTED_TX_KIND: &str = "arg.unsupported_tx_kind";
const CODE_SUBMIT_TX_ALREADY_SEEN: &str = "submit.tx_already_seen";
const CODE_SUBMIT_INVALID_FEE: &str = "submit.invalid_fee";
const CODE_SUBMIT_NONCE_TOO_LOW: &str = "submit.nonce_too_low";
const CODE_SUBMIT_NONCE_GAP: &str = "submit.nonce_gap";
const CODE_SUBMIT_NONCE_CONFLICT: &str = "submit.nonce_conflict";
const CODE_SUBMIT_QUEUE_FULL: &str = "submit.queue_full";
const CODE_SUBMIT_SENDER_QUEUE_FULL: &str = "submit.sender_queue_full";
const CODE_SUBMIT_PRINCIPAL_QUEUE_FULL: &str = "submit.principal_queue_full";
const CODE_SUBMIT_DECODE_RATE_LIMITED: &str = "submit.decode_rate_limited";
const CODE_INTERNAL_UNEXPECTED: &str = "internal.unexpected";

pub fn rpc_eth_get_block_by_number_with_status(number: u64, full_tx: bool) -> RpcBlockLookupView {
    if let Some(pruned) = prune_boundary_for_number(number) {
        return RpcBlockLookupView::Pruned {
            pruned_before_block: pruned,
        };
    }
    let Some(block) = chain::get_block(number) else {
        return RpcBlockLookupView::NotFound;
    };
    RpcBlockLookupView::Found(block_to_eth_view(block, full_tx))
}

pub fn rpc_eth_get_transaction_by_eth_hash(eth_tx_hash: Vec<u8>) -> Option<EthTxView> {
    let tx_id = find_eth_tx_id_by_eth_hash_bytes(&eth_tx_hash)?;
    tx_to_view(tx_id)
}

pub fn rpc_eth_get_transaction_by_tx_id(tx_id: Vec<u8>) -> Option<EthTxView> {
    let parsed_tx_id = tx_id_from_bytes(tx_id)?;
    tx_to_view(parsed_tx_id)
}

pub fn rpc_eth_get_transaction_receipt_by_eth_hash(eth_tx_hash: Vec<u8>) -> Option<EthReceiptView> {
    let tx_id = find_eth_tx_id_by_eth_hash_bytes(&eth_tx_hash)?;
    chain::get_receipt(&tx_id).map(receipt_to_eth_view)
}

pub fn rpc_eth_get_transaction_receipt_with_status(tx_hash_or_id: Vec<u8>) -> RpcReceiptLookupView {
    let Some(tx_id) = find_eth_tx_id_by_eth_hash_bytes(&tx_hash_or_id)
        .or_else(|| tx_id_from_bytes(tx_hash_or_id))
    else {
        return RpcReceiptLookupView::NotFound;
    };
    receipt_lookup_status(tx_id)
}

pub fn rpc_eth_get_balance(address: Vec<u8>) -> Result<Vec<u8>, String> {
    let addr = parse_address_20_with_label(address, "address")?;
    let key = make_account_key(addr);
    let balance = with_state(|state| {
        state
            .accounts
            .get(&key)
            .map(|value| value.balance().to_vec())
            .unwrap_or_else(|| [0u8; 32].to_vec())
    });
    Ok(balance)
}

pub fn rpc_eth_get_code(address: Vec<u8>) -> Result<Vec<u8>, String> {
    let addr = parse_address_20_with_label(address, "address")?;
    let key = make_account_key(addr);
    let code = with_state(|state| {
        let Some(account) = state.accounts.get(&key) else {
            return Vec::new();
        };
        let code_hash = account.code_hash();
        if code_hash == [0u8; 32] {
            return Vec::new();
        }
        state
            .codes
            .get(&make_code_key(code_hash))
            .map(|value| value.0)
            .unwrap_or_default()
    });
    Ok(code)
}

pub fn rpc_eth_get_storage_at(address: Vec<u8>, slot: Vec<u8>) -> Result<Vec<u8>, String> {
    let addr = parse_address_20_with_label(address, "address")?;
    let slot32 = parse_hash_32(slot).ok_or_else(|| "slot must be 32 bytes".to_string())?;
    let key = make_storage_key(addr, slot32);
    let value = with_state(|state| {
        state
            .storage
            .get(&key)
            .map(|v| v.0.to_vec())
            .unwrap_or_else(|| [0u8; 32].to_vec())
    });
    Ok(value)
}

const RPC_ERR_INVALID_PARAMS: u32 = 1001;
const RPC_ERR_EXECUTION_FAILED: u32 = 2001;
const MAX_FEE_HISTORY_BLOCKS: u64 = 256;
const EIP1559_BASE_FEE_MAX_CHANGE_DENOM: u128 = 8;
const EIP1559_ELASTICITY_MULTIPLIER: u128 = 2;

fn rpc_error(code: u32, prefix: Option<&str>, message: impl Into<String>) -> RpcErrorView {
    RpcErrorView {
        code,
        message: message.into(),
        error_prefix: prefix.map(str::to_string),
    }
}

fn invalid_error(prefix: &str, message: impl Into<String>) -> RpcErrorView {
    rpc_error(RPC_ERR_INVALID_PARAMS, Some(prefix), message)
}

fn execution_error(prefix: &str, message: impl Into<String>) -> RpcErrorView {
    rpc_error(RPC_ERR_EXECUTION_FAILED, Some(prefix), message)
}

pub fn rpc_eth_call_object(call: RpcCallObjectView) -> Result<RpcCallResultView, RpcErrorView> {
    let input =
        call_object_to_input(call).map_err(|message| invalid_error("invalid.call_object", message))?;
    let out = chain::eth_call_object(input)
        .map_err(|err| execution_error("exec.eth_call_object.failed", format!("eth_call_object failed: {err:?}")))?;
    Ok(RpcCallResultView {
        status: out.status,
        gas_used: out.gas_used,
        return_data: out.return_data,
        revert_data: out.revert_data,
    })
}

pub fn rpc_eth_estimate_gas_object(call: RpcCallObjectView) -> Result<u64, RpcErrorView> {
    let input =
        call_object_to_input(call).map_err(|message| invalid_error("invalid.call_object", message))?;
    chain::eth_estimate_gas_object(input).map_err(|err| {
        execution_error(
            "exec.eth_estimate_gas_object.failed",
            format!("eth_estimate_gas_object failed: {err:?}"),
        )
    })
}

pub fn rpc_eth_get_transaction_count_at(
    address: Vec<u8>,
    tag: RpcBlockTagView,
) -> Result<u64, RpcErrorView> {
    let sender = parse_address_20_with_label(address, "address")
        .map_err(|message| invalid_error("invalid.address", message))?;
    let latest_nonce = || {
        let key = make_account_key(sender);
        with_state(|state| {
            state
                .accounts
                .get(&key)
                .map(|value| value.nonce())
                .unwrap_or(0)
        })
    };
    match tag {
        RpcBlockTagView::Pending => Ok(chain::expected_nonce_for_sender_view(sender)),
        RpcBlockTagView::Latest | RpcBlockTagView::Safe | RpcBlockTagView::Finalized => {
            Ok(latest_nonce())
        }
        RpcBlockTagView::Earliest => {
            let window = rpc_eth_history_window();
            if window.oldest_available > 0 {
                return Err(out_of_window_error(0, window));
            }
            Err(execution_error(
                "exec.state.unavailable",
                "exec.state.unavailable historical nonce is unavailable for earliest",
            ))
        }
        RpcBlockTagView::Number(number) => {
            let window = rpc_eth_history_window();
            if number < window.oldest_available || number > window.latest {
                return Err(out_of_window_error(number, window));
            }
            Err(execution_error(
                "exec.state.unavailable",
                format!(
                    "exec.state.unavailable historical nonce is unavailable requested={number}"
                ),
            ))
        }
    }
}

pub fn rpc_eth_call_object_at(
    call: RpcCallObjectView,
    tag: RpcBlockTagView,
) -> Result<RpcCallResultView, RpcErrorView> {
    match tag {
        RpcBlockTagView::Latest
        | RpcBlockTagView::Pending
        | RpcBlockTagView::Safe
        | RpcBlockTagView::Finalized => rpc_eth_call_object(call),
        RpcBlockTagView::Earliest => unsupported_historical_exec_call(0),
        RpcBlockTagView::Number(number) => unsupported_historical_exec_call(number),
    }
}

pub fn rpc_eth_estimate_gas_object_at(
    call: RpcCallObjectView,
    tag: RpcBlockTagView,
) -> Result<u64, RpcErrorView> {
    match tag {
        RpcBlockTagView::Latest
        | RpcBlockTagView::Pending
        | RpcBlockTagView::Safe
        | RpcBlockTagView::Finalized => rpc_eth_estimate_gas_object(call),
        RpcBlockTagView::Earliest => unsupported_historical_exec_gas(0),
        RpcBlockTagView::Number(number) => unsupported_historical_exec_gas(number),
    }
}

pub fn rpc_eth_max_priority_fee_per_gas() -> Result<u128, RpcErrorView> {
    let head = chain::get_head_number();
    let sample = load_fee_history_sample(head).ok_or_else(|| {
        execution_error("exec.state.unavailable", "exec.state.unavailable fee sample is unavailable")
    })?;
    let median = compute_weighted_percentile(&sample.tx_tips, 50.0);
    if median > 0 {
        return Ok(median);
    }
    let min_positive = sample
        .tx_tips
        .iter()
        .filter(|item| item.tip > 0)
        .map(|item| item.tip)
        .min()
        .unwrap_or(0);
    Ok(min_positive)
}

pub fn rpc_eth_fee_history(
    block_count: u64,
    newest: RpcBlockTagView,
    reward_percentiles: Option<Vec<f64>>,
) -> Result<RpcFeeHistoryView, RpcErrorView> {
    if block_count == 0 || block_count > MAX_FEE_HISTORY_BLOCKS {
        return Err(invalid_error(
            "invalid.fee_history.block_count",
            format!("invalid.fee_history.block_count block_count must be within [1, {MAX_FEE_HISTORY_BLOCKS}]"),
        ));
    }
    let percentiles = validate_reward_percentiles(reward_percentiles)?;
    let newest_number = resolve_newest_number(newest)?;
    let mut samples = Vec::new();
    for offset in 0..block_count {
        let number = newest_number.saturating_sub(offset);
        let Some(sample) = load_fee_history_sample(number) else {
            break;
        };
        samples.push(sample);
        if number == 0 {
            break;
        }
    }
    samples.sort_by_key(|item| item.number);
    if samples.is_empty() {
        return Err(execution_error(
            "exec.state.unavailable",
            "exec.state.unavailable fee history is unavailable",
        ));
    }
    let last = samples.last().ok_or_else(|| {
        execution_error("exec.state.unavailable", "exec.state.unavailable fee history is unavailable")
    })?;
    let mut base_fee_per_gas: Vec<u64> = samples.iter().map(|item| item.base_fee_per_gas).collect();
    base_fee_per_gas.push(compute_next_base_fee(
        last.base_fee_per_gas,
        last.gas_used,
        last.gas_limit,
    ));
    let gas_used_ratio = samples
        .iter()
        .map(|item| {
            if item.gas_limit == 0 {
                0.0
            } else {
                item.gas_used as f64 / item.gas_limit as f64
            }
        })
        .collect();
    let reward = percentiles.map(|ps| {
        samples
            .iter()
            .map(|item| {
                ps.iter()
                    .map(|p| compute_weighted_percentile(&item.tx_tips, *p))
                    .collect()
            })
            .collect()
    });
    Ok(RpcFeeHistoryView {
        oldest_block: samples[0].number,
        base_fee_per_gas,
        gas_used_ratio,
        reward,
    })
}

pub fn rpc_eth_history_window() -> RpcHistoryWindowView {
    let latest = chain::get_head_number();
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    RpcHistoryWindowView {
        oldest_available: pruned_before.map(|v| v.saturating_add(1)).unwrap_or(0),
        latest,
    }
}

pub fn rpc_eth_call_rawtx(raw_tx: Vec<u8>) -> Result<Vec<u8>, String> {
    chain::eth_call(raw_tx).map_err(|err| format!("eth_call failed: {err:?}"))
}

pub fn rpc_eth_send_raw_transaction(
    raw_tx: Vec<u8>,
    caller_principal: Vec<u8>,
) -> Result<Vec<u8>, SubmitTxError> {
    submit_tx_in_with_code(
        chain::TxIn::EthSigned {
            tx_bytes: raw_tx,
            caller_principal,
        },
        "rpc_eth_send_raw_transaction",
    )
}

pub fn submit_tx_in_with_code(tx_in: chain::TxIn, op_name: &str) -> Result<Vec<u8>, SubmitTxError> {
    match chain::submit_tx_in(tx_in) {
        Ok(tx_id) => Ok(tx_id.0.to_vec()),
        Err(err) => Err(map_submit_chain_error(err, op_name)),
    }
}

pub fn rpc_eth_get_logs_paged(
    filter: EthLogFilterView,
    cursor: Option<EthLogsCursorView>,
    limit: u32,
) -> Result<EthLogsPageView, GetLogsErrorView> {
    const DEFAULT_LIMIT: usize = 200;
    const MAX_LIMIT: usize = 2000;
    const MAX_BLOCK_SPAN: u64 = 1000;
    const MAX_SCANNED_RECEIPTS: usize = 20_000;

    if filter.topic1.is_some() {
        return Err(GetLogsErrorView::UnsupportedFilter(
            "topic1 is not supported".to_string(),
        ));
    }

    let head = chain::get_head_number();
    let mut from = filter.from_block.unwrap_or(0);
    let mut to = filter.to_block.unwrap_or(head);
    if from > to {
        return Err(GetLogsErrorView::InvalidArgument(
            "from_block must be <= to_block".to_string(),
        ));
    }
    if to.saturating_sub(from) > MAX_BLOCK_SPAN {
        return Err(GetLogsErrorView::RangeTooLarge);
    }
    if to > head {
        to = head;
    }

    let requested_limit_u32 = if limit == 0 {
        filter
            .limit
            .unwrap_or(u32::try_from(DEFAULT_LIMIT).unwrap_or(u32::MAX))
    } else {
        limit
    }
    .max(1);
    let requested_limit = usize::try_from(requested_limit_u32).unwrap_or(usize::MAX);
    if requested_limit > MAX_LIMIT {
        return Err(GetLogsErrorView::TooManyResults);
    }

    let address_filter = match filter.address {
        Some(bytes) => Some(
            parse_address_20_with_label(bytes, "address")
                .map_err(GetLogsErrorView::InvalidArgument)?,
        ),
        None => None,
    };
    let topic0_filter = match filter.topic0 {
        Some(bytes) => Some(parse_hash_32(bytes).ok_or_else(|| {
            GetLogsErrorView::InvalidArgument("topic0 must be 32 bytes".to_string())
        })?),
        None => None,
    };

    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    if let Some(pruned) = pruned_before {
        if from <= pruned {
            from = pruned.saturating_add(1);
        }
    }

    let mut out = Vec::new();
    let mut scanned_receipts = 0usize;
    let mut start_block = from;
    let mut start_tx_index: usize = 0;
    let mut start_log_index: usize = 0;
    if let Some(c) = cursor {
        if c.block_number < from || c.block_number > to {
            return Err(GetLogsErrorView::InvalidArgument(
                "cursor out of range".to_string(),
            ));
        }
        start_block = c.block_number;
        start_tx_index = usize::try_from(c.tx_index).unwrap_or(0);
        start_log_index = usize::try_from(c.log_index).unwrap_or(0);
    }

    for number in start_block..=to {
        let Some(block) = chain::get_block(number) else {
            continue;
        };
        let tx_start = if number == start_block {
            start_tx_index
        } else {
            0
        };
        for (tx_pos, tx_id) in block.tx_ids.iter().enumerate().skip(tx_start) {
            if scanned_receipts >= MAX_SCANNED_RECEIPTS {
                return Ok(EthLogsPageView {
                    items: out,
                    next_cursor: cursor_after_scan_limit(number, tx_pos, block.tx_ids.len(), to),
                });
            }
            let Some(receipt) = chain::get_receipt(tx_id) else {
                continue;
            };
            scanned_receipts = scanned_receipts.saturating_add(1);
            let eth_tx_hash = chain::get_tx_envelope(tx_id)
                .and_then(|envelope| StoredTx::try_from(envelope).ok())
                .and_then(|stored| {
                    if stored.kind == TxKind::EthSigned {
                        Some(hash::keccak256(&stored.raw).to_vec())
                    } else {
                        None
                    }
                });
            let log_start = if number == start_block && tx_pos == tx_start {
                start_log_index
            } else {
                0
            };
            for (log_index, log) in receipt.logs.iter().enumerate().skip(log_start) {
                let address = log.address.as_slice();
                if let Some(filter_addr) = address_filter {
                    if address != filter_addr {
                        continue;
                    }
                }
                if let Some(topic0) = topic0_filter {
                    let topics = log.data.topics();
                    if topics.is_empty() || topics[0].as_slice() != topic0 {
                        continue;
                    }
                }
                if out.len() == requested_limit {
                    return Ok(EthLogsPageView {
                        items: out,
                        next_cursor: Some(EthLogsCursorView {
                            block_number: number,
                            tx_index: u32::try_from(tx_pos).unwrap_or(u32::MAX),
                            log_index: u32::try_from(log_index.saturating_add(1))
                                .unwrap_or(u32::MAX),
                        }),
                    });
                }
                out.push(EthLogItemView {
                    block_number: receipt.block_number,
                    tx_index: receipt.tx_index,
                    log_index: u32::try_from(log_index).unwrap_or(u32::MAX),
                    tx_hash: receipt.tx_id.0.to_vec(),
                    eth_tx_hash: eth_tx_hash.clone(),
                    address: address.to_vec(),
                    topics: log
                        .data
                        .topics()
                        .iter()
                        .map(|topic| topic.as_slice().to_vec())
                        .collect(),
                    data: log.data.data.to_vec(),
                });
            }
        }
    }
    Ok(EthLogsPageView {
        items: out,
        next_cursor: None,
    })
}

fn cursor_after_scan_limit(
    block_number: u64,
    tx_index: usize,
    tx_len: usize,
    to_block: u64,
) -> Option<EthLogsCursorView> {
    let next_tx = tx_index.saturating_add(1);
    if next_tx < tx_len {
        return Some(EthLogsCursorView {
            block_number,
            tx_index: u32::try_from(next_tx).unwrap_or(u32::MAX),
            log_index: 0,
        });
    }
    let next_block = block_number.saturating_add(1);
    if next_block <= to_block {
        return Some(EthLogsCursorView {
            block_number: next_block,
            tx_index: 0,
            log_index: 0,
        });
    }
    None
}

fn chain_submit_error_to_code(err: &chain::ChainError) -> Option<(TxApiErrorKind, &'static str)> {
    match err {
        chain::ChainError::TxTooLarge => {
            Some((TxApiErrorKind::InvalidArgument, CODE_ARG_TX_TOO_LARGE))
        }
        chain::ChainError::DecodeFailed => {
            Some((TxApiErrorKind::InvalidArgument, CODE_ARG_DECODE_FAILED))
        }
        chain::ChainError::AddressDerivationFailed => {
            Some((TxApiErrorKind::InvalidArgument, CODE_ARG_DERIVATION_FAILED))
        }
        chain::ChainError::UnsupportedTxKind => Some((
            TxApiErrorKind::InvalidArgument,
            CODE_ARG_UNSUPPORTED_TX_KIND,
        )),
        chain::ChainError::TxAlreadySeen => {
            Some((TxApiErrorKind::Rejected, CODE_SUBMIT_TX_ALREADY_SEEN))
        }
        chain::ChainError::InvalidFee => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_INVALID_FEE)),
        chain::ChainError::NonceTooLow => {
            Some((TxApiErrorKind::Rejected, CODE_SUBMIT_NONCE_TOO_LOW))
        }
        chain::ChainError::NonceGap => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_NONCE_GAP)),
        chain::ChainError::NonceConflict => {
            Some((TxApiErrorKind::Rejected, CODE_SUBMIT_NONCE_CONFLICT))
        }
        chain::ChainError::QueueFull => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_QUEUE_FULL)),
        chain::ChainError::SenderQueueFull => {
            Some((TxApiErrorKind::Rejected, CODE_SUBMIT_SENDER_QUEUE_FULL))
        }
        chain::ChainError::PrincipalQueueFull => {
            Some((TxApiErrorKind::Rejected, CODE_SUBMIT_PRINCIPAL_QUEUE_FULL))
        }
        chain::ChainError::DecodeRateLimited => {
            Some((TxApiErrorKind::Rejected, CODE_SUBMIT_DECODE_RATE_LIMITED))
        }
        _ => None,
    }
}

fn map_submit_chain_error(err: chain::ChainError, op_name: &str) -> SubmitTxError {
    if let Some((kind, code)) = chain_submit_error_to_code(&err) {
        return match kind {
            TxApiErrorKind::InvalidArgument => SubmitTxError::InvalidArgument(code.to_string()),
            TxApiErrorKind::Rejected => SubmitTxError::Rejected(code.to_string()),
        };
    }
    warn!(operation = op_name, error = ?err, "submit failed with unmapped chain error");
    error!(operation = op_name, error = ?err, "submit failed");
    SubmitTxError::Internal(CODE_INTERNAL_UNEXPECTED.to_string())
}

fn tx_id_from_bytes(tx_id: Vec<u8>) -> Option<TxId> {
    if tx_id.len() != 32 {
        return None;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    Some(TxId(buf))
}

fn parse_address_20_with_label(bytes: Vec<u8>, label: &str) -> Result<[u8; 20], String> {
    if bytes.len() != 20 {
        return Err(address_len_error(label, bytes.len()));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Ok(out)
}

fn address_len_error(label: &str, len: usize) -> String {
    if len == 32 {
        return format!(
            "{label} must be 20 bytes (got 32; this looks like bytes32-encoded principal)"
        );
    }
    format!("{label} must be 20 bytes")
}

fn parse_hash_32(bytes: Vec<u8>) -> Option<[u8; 32]> {
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

fn call_object_to_input(call: RpcCallObjectView) -> Result<chain::CallObjectInput, String> {
    if call.gas_price.is_some()
        && (call.max_fee_per_gas.is_some() || call.max_priority_fee_per_gas.is_some())
    {
        return Err(
            "gasPrice and maxFeePerGas/maxPriorityFeePerGas cannot be used together".to_string(),
        );
    }
    if call.max_priority_fee_per_gas.is_some() && call.max_fee_per_gas.is_none() {
        return Err("maxPriorityFeePerGas requires maxFeePerGas".to_string());
    }
    if let (Some(priority), Some(max_fee)) = (call.max_priority_fee_per_gas, call.max_fee_per_gas) {
        if priority > max_fee {
            return Err("maxPriorityFeePerGas must be <= maxFeePerGas".to_string());
        }
    }
    let tx_type = match call.tx_type {
        Some(0) => Some(0u8),
        Some(2) => Some(2u8),
        Some(_) => return Err("type must be 0x0 or 0x2".to_string()),
        None => None,
    };
    if matches!(tx_type, Some(0))
        && (call.max_fee_per_gas.is_some() || call.max_priority_fee_per_gas.is_some())
    {
        return Err("type=0 cannot be used with maxFeePerGas/maxPriorityFeePerGas".to_string());
    }
    if matches!(tx_type, Some(2)) && call.gas_price.is_some() {
        return Err("type=2 cannot be used with gasPrice".to_string());
    }
    if let Some(chain_id) = call.chain_id {
        if chain_id != CHAIN_ID {
            return Err(format!(
                "chainId mismatch: expected {CHAIN_ID}, got {chain_id}"
            ));
        }
    }
    let to = match call.to {
        Some(bytes) => Some(parse_address_20_with_label(bytes, "to")?),
        None => None,
    };
    let from = match call.from {
        Some(bytes) => parse_address_20_with_label(bytes, "from")?,
        None => [0u8; 20],
    };
    let value = match call.value {
        Some(bytes) => parse_hash_32(bytes).ok_or_else(|| "value must be 32 bytes".to_string())?,
        None => [0u8; 32],
    };
    let access_list = match call.access_list {
        Some(items) => parse_access_list(items)?,
        None => Vec::new(),
    };
    let data = call.data.unwrap_or_default();
    Ok(chain::CallObjectInput {
        to,
        from,
        gas_limit: call.gas,
        gas_price: call.gas_price,
        nonce: call.nonce,
        max_fee_per_gas: call.max_fee_per_gas,
        max_priority_fee_per_gas: call.max_priority_fee_per_gas,
        chain_id: call.chain_id,
        tx_type,
        access_list,
        value,
        data,
    })
}

fn parse_access_list(
    items: Vec<RpcAccessListItemView>,
) -> Result<Vec<([u8; 20], Vec<[u8; 32]>)>, String> {
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let address = parse_address_20_with_label(item.address, "accessList.address")?;
        let mut storage_keys = Vec::with_capacity(item.storage_keys.len());
        for key in item.storage_keys {
            storage_keys.push(
                parse_hash_32(key)
                    .ok_or_else(|| "accessList.storageKeys[] must be 32 bytes".to_string())?,
            );
        }
        out.push((address, storage_keys));
    }
    Ok(out)
}

fn find_eth_tx_id_by_eth_hash_bytes(eth_tx_hash: &[u8]) -> Option<TxId> {
    if eth_tx_hash.len() != 32 {
        return None;
    }
    let mut hash_buf = [0u8; 32];
    hash_buf.copy_from_slice(eth_tx_hash);
    let hash_key = TxId(hash_buf);
    with_state(|state| state.eth_tx_hash_index.get(&hash_key))
}

fn tx_to_view(tx_id: TxId) -> Option<EthTxView> {
    let envelope = chain::get_tx_envelope(&tx_id)?;
    let (block_number, tx_index) = match chain::get_tx_loc(&tx_id) {
        Some(TxLoc {
            kind: TxLocKind::Included,
            block_number,
            tx_index,
            ..
        }) => (Some(block_number), Some(tx_index)),
        _ => (None, None),
    };
    envelope_to_eth_view(envelope, block_number, tx_index)
}

fn eth_hash_or_tx_id(tx_id: TxId) -> Vec<u8> {
    let Some(envelope) = chain::get_tx_envelope(&tx_id) else {
        return tx_id.0.to_vec();
    };
    let Ok(stored) = StoredTx::try_from(envelope) else {
        return tx_id.0.to_vec();
    };
    if stored.kind == TxKind::EthSigned {
        return hash::keccak256(&stored.raw).to_vec();
    }
    tx_id.0.to_vec()
}

fn envelope_to_eth_view(
    envelope: StoredTxBytes,
    block_number: Option<u64>,
    tx_index: Option<u32>,
) -> Option<EthTxView> {
    let stored = StoredTx::try_from(envelope).ok()?;
    let kind = stored.kind;
    let caller = match kind {
        TxKind::IcSynthetic => stored.caller_evm.unwrap_or([0u8; 20]),
        TxKind::EthSigned => [0u8; 20],
    };
    let decoded =
        if let Ok(decoded) = evm_core::tx_decode::decode_tx_view(kind, caller, &stored.raw) {
            Some(DecodedTxView {
                from: decoded.from.to_vec(),
                to: decoded.to.map(|addr| addr.to_vec()),
                nonce: decoded.nonce,
                value: decoded.value.to_vec(),
                input: decoded.input.into_owned(),
                gas_limit: decoded.gas_limit,
                gas_price: decoded.gas_price,
                max_fee_per_gas: decoded.max_fee_per_gas,
                max_priority_fee_per_gas: decoded.max_priority_fee_per_gas,
                chain_id: decoded.chain_id,
            })
        } else {
            None
        };

    Some(EthTxView {
        hash: stored.tx_id.0.to_vec(),
        eth_tx_hash: if kind == TxKind::EthSigned {
            Some(hash::keccak256(&stored.raw).to_vec())
        } else {
            None
        },
        caller_principal: if stored.caller_principal.is_empty() {
            None
        } else {
            Some(stored.caller_principal.clone())
        },
        kind: tx_kind_to_view(kind),
        raw: stored.raw.clone(),
        decode_ok: decoded.is_some(),
        decoded,
        block_number,
        tx_index,
    })
}

fn receipt_to_eth_view(receipt: ReceiptLike) -> EthReceiptView {
    let eth_tx_hash = chain::get_tx_envelope(&receipt.tx_id)
        .and_then(|envelope| StoredTx::try_from(envelope).ok())
        .and_then(|stored| {
            if stored.kind == TxKind::EthSigned {
                Some(hash::keccak256(&stored.raw).to_vec())
            } else {
                None
            }
        });
    let base_log_index = base_log_index_for_receipt(&receipt);
    EthReceiptView {
        tx_hash: receipt.tx_id.0.to_vec(),
        eth_tx_hash,
        block_number: receipt.block_number,
        tx_index: receipt.tx_index,
        status: receipt.status,
        gas_used: receipt.gas_used,
        effective_gas_price: receipt.effective_gas_price,
        l1_data_fee: receipt.l1_data_fee,
        operator_fee: receipt.operator_fee,
        total_fee: receipt.total_fee,
        contract_address: receipt.contract_address.map(|v| v.to_vec()),
        logs: receipt
            .logs
            .into_iter()
            .enumerate()
            .map(|(idx, log)| EthReceiptLogView {
                address: log.address.as_slice().to_vec(),
                topics: log
                    .data
                    .topics()
                    .iter()
                    .map(|topic| topic.as_slice().to_vec())
                    .collect(),
                data: log.data.data.to_vec(),
                log_index: base_log_index.saturating_add(u32::try_from(idx).unwrap_or(u32::MAX)),
            })
            .collect(),
    }
}

fn base_log_index_for_receipt(receipt: &ReceiptLike) -> u32 {
    let Some(block) = chain::get_block(receipt.block_number) else {
        warn!(
            block_number = receipt.block_number,
            "missing block for receipt log index"
        );
        return 0;
    };
    let mut offset: u64 = 0;
    for tx_id in &block.tx_ids {
        if *tx_id == receipt.tx_id {
            return u32::try_from(offset).unwrap_or(u32::MAX);
        }
        if let Some(prev_receipt) = chain::get_receipt(tx_id) {
            offset =
                offset.saturating_add(u64::try_from(prev_receipt.logs.len()).unwrap_or(u64::MAX));
        }
    }
    warn!(
        block_number = receipt.block_number,
        tx_id = ?receipt.tx_id,
        "tx not found in block while computing receipt log index"
    );
    0
}

fn tx_kind_to_view(kind: TxKind) -> TxKindView {
    match kind {
        TxKind::EthSigned => TxKindView::EthSigned,
        TxKind::IcSynthetic => TxKindView::IcSynthetic,
    }
}

fn block_to_eth_view(block: BlockData, full_tx: bool) -> EthBlockView {
    let txs = if full_tx {
        let mut list = Vec::with_capacity(block.tx_ids.len());
        for tx_id in &block.tx_ids {
            if let Some(view) = tx_to_view(*tx_id) {
                list.push(view);
            }
        }
        EthTxListView::Full(list)
    } else {
        EthTxListView::Hashes(
            block
                .tx_ids
                .iter()
                .map(|id| eth_hash_or_tx_id(*id))
                .collect(),
        )
    };
    EthBlockView {
        number: block.number,
        parent_hash: block.parent_hash.to_vec(),
        block_hash: block.block_hash.to_vec(),
        timestamp: block.timestamp,
        beneficiary: block.beneficiary.to_vec(),
        txs,
        state_root: block.state_root.to_vec(),
        base_fee_per_gas: Some(block.base_fee_per_gas),
        gas_limit: Some(block.block_gas_limit),
        gas_used: Some(block.gas_used),
    }
}

#[derive(Clone, Debug)]
struct FeeTipSample {
    tip: u128,
    gas_used: u64,
}

#[derive(Clone, Debug)]
struct FeeHistorySample {
    number: u64,
    base_fee_per_gas: u64,
    gas_used: u64,
    gas_limit: u64,
    tx_tips: Vec<FeeTipSample>,
}

fn load_fee_history_sample(number: u64) -> Option<FeeHistorySample> {
    let block = chain::get_block(number)?;
    let mut tx_tips = Vec::new();
    for tx_id in &block.tx_ids {
        let Some(tx) = tx_to_view(*tx_id) else {
            continue;
        };
        let Some(decoded) = tx.decoded else {
            continue;
        };
        let gas_used = chain::get_receipt(tx_id)
            .map(|r| r.gas_used)
            .unwrap_or(decoded.gas_limit);
        if gas_used == 0 {
            continue;
        }
        let tip = effective_priority_fee(&decoded, block.base_fee_per_gas as u128);
        tx_tips.push(FeeTipSample { tip, gas_used });
    }
    Some(FeeHistorySample {
        number: block.number,
        base_fee_per_gas: block.base_fee_per_gas,
        gas_used: block.gas_used,
        gas_limit: block.block_gas_limit,
        tx_tips,
    })
}

fn effective_priority_fee(decoded: &DecodedTxView, base_fee: u128) -> u128 {
    if let Some(max_fee) = decoded.max_fee_per_gas {
        let cap_by_base = max_fee.saturating_sub(base_fee);
        if let Some(max_priority) = decoded.max_priority_fee_per_gas {
            return max_priority.min(cap_by_base);
        }
        return cap_by_base;
    }
    if let Some(gas_price) = decoded.gas_price {
        return gas_price.saturating_sub(base_fee);
    }
    0
}

fn compute_weighted_percentile(items: &[FeeTipSample], percentile: f64) -> u128 {
    if items.is_empty() {
        return 0;
    }
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| a.tip.cmp(&b.tip).then(a.gas_used.cmp(&b.gas_used)));
    let total_weight = sorted
        .iter()
        .fold(0u128, |acc, item| acc.saturating_add(item.gas_used as u128));
    if total_weight == 0 {
        return 0;
    }
    let threshold = percentile_to_threshold(total_weight, percentile);
    let mut cumulative = 0u128;
    for item in sorted {
        cumulative = cumulative.saturating_add(item.gas_used as u128);
        if cumulative >= threshold {
            return item.tip;
        }
    }
    0
}

fn percentile_to_threshold(total_weight: u128, percentile: f64) -> u128 {
    if percentile <= 0.0 {
        return 1;
    }
    if percentile >= 100.0 {
        return total_weight;
    }
    let scaled = (percentile * 1_000_000.0).round() as u128;
    let divisor = 100_000_000u128;
    let numerator = total_weight.saturating_mul(scaled);
    let ceil = numerator.saturating_add(divisor - 1) / divisor;
    ceil.max(1)
}

fn compute_next_base_fee(base_fee: u64, gas_used: u64, gas_limit: u64) -> u64 {
    if gas_limit == 0 {
        return base_fee;
    }
    let target_gas = gas_limit as u128 / EIP1559_ELASTICITY_MULTIPLIER;
    if target_gas == 0 {
        return base_fee;
    }
    let base = base_fee as u128;
    let used = gas_used as u128;
    if used == target_gas {
        return base_fee;
    }
    if used > target_gas {
        let gas_delta = used - target_gas;
        let base_delta = base
            .saturating_mul(gas_delta)
            .saturating_div(target_gas)
            .saturating_div(EIP1559_BASE_FEE_MAX_CHANGE_DENOM);
        return (base.saturating_add(base_delta.max(1))).min(u64::MAX as u128) as u64;
    }
    let gas_delta = target_gas - used;
    let base_delta = base
        .saturating_mul(gas_delta)
        .saturating_div(target_gas)
        .saturating_div(EIP1559_BASE_FEE_MAX_CHANGE_DENOM);
    base.saturating_sub(base_delta).min(u64::MAX as u128) as u64
}

fn validate_reward_percentiles(
    reward_percentiles: Option<Vec<f64>>,
) -> Result<Option<Vec<f64>>, RpcErrorView> {
    let Some(percentiles) = reward_percentiles else {
        return Ok(None);
    };
    let mut prev = -1.0f64;
    for value in &percentiles {
        if !value.is_finite() || *value < 0.0 || *value > 100.0 {
            return Err(invalid_error(
                "invalid.fee_history.percentiles",
                "invalid.fee_history.percentiles percentile must be within [0,100]",
            ));
        }
        if *value < prev {
            return Err(invalid_error(
                "invalid.fee_history.percentiles",
                "invalid.fee_history.percentiles percentiles must be monotonically increasing",
            ));
        }
        prev = *value;
    }
    Ok(Some(percentiles))
}

fn resolve_newest_number(tag: RpcBlockTagView) -> Result<u64, RpcErrorView> {
    let window = rpc_eth_history_window();
    let head = window.latest;
    match tag {
        RpcBlockTagView::Latest
        | RpcBlockTagView::Pending
        | RpcBlockTagView::Safe
        | RpcBlockTagView::Finalized => Ok(head),
        RpcBlockTagView::Earliest => {
            if window.oldest_available > 0 {
                return Err(out_of_window_error(0, window));
            }
            Ok(0)
        }
        RpcBlockTagView::Number(number) => {
            if number < window.oldest_available || number > head {
                return Err(out_of_window_error(number, window));
            }
            Ok(number)
        }
    }
}

fn unsupported_historical_exec_err(number: u64) -> RpcErrorView {
    let window = rpc_eth_history_window();
    if number < window.oldest_available || number > window.latest {
        return out_of_window_error(number, window);
    }
    execution_error(
        "exec.state.unavailable",
        format!(
            "exec.state.unavailable historical execution is unavailable requested={} oldest_available={} latest={}",
            number, window.oldest_available, window.latest
        ),
    )
}

fn unsupported_historical_exec_call(number: u64) -> Result<RpcCallResultView, RpcErrorView> {
    Err(unsupported_historical_exec_err(number))
}

fn unsupported_historical_exec_gas(number: u64) -> Result<u64, RpcErrorView> {
    Err(unsupported_historical_exec_err(number))
}

fn out_of_window_error(requested: u64, window: RpcHistoryWindowView) -> RpcErrorView {
    invalid_error(
        "invalid.block_range.out_of_window",
        format!(
            "invalid.block_range.out_of_window requested={} oldest_available={} latest={}",
            requested, window.oldest_available, window.latest
        ),
    )
}

fn prune_boundary_for_number(number: u64) -> Option<u64> {
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    match pruned_before {
        Some(pruned) if number <= pruned => Some(pruned),
        _ => None,
    }
}

fn receipt_lookup_status(tx_id: TxId) -> RpcReceiptLookupView {
    if let Some(receipt) = chain::get_receipt(&tx_id) {
        return RpcReceiptLookupView::Found(receipt_to_eth_view(receipt));
    }
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    let loc = chain::get_tx_loc(&tx_id);
    if let Some(loc) = loc {
        if loc.kind == TxLocKind::Included {
            if let Some(pruned) = pruned_before {
                if loc.block_number <= pruned {
                    return RpcReceiptLookupView::Pruned {
                        pruned_before_block: pruned,
                    };
                }
            }
        }
        return RpcReceiptLookupView::NotFound;
    }
    if let Some(pruned) = pruned_before {
        return RpcReceiptLookupView::PossiblyPruned {
            pruned_before_block: pruned,
        };
    }
    RpcReceiptLookupView::NotFound
}

#[cfg(test)]
mod tests {
    use super::cursor_after_scan_limit;

    #[test]
    fn cursor_after_scan_limit_stays_in_block_when_txs_remain() {
        let out = cursor_after_scan_limit(10, 3, 8, 20).expect("cursor");
        assert_eq!(out.block_number, 10);
        assert_eq!(out.tx_index, 4);
        assert_eq!(out.log_index, 0);
    }

    #[test]
    fn cursor_after_scan_limit_moves_to_next_block_at_tx_end() {
        let out = cursor_after_scan_limit(10, 7, 8, 20).expect("cursor");
        assert_eq!(out.block_number, 11);
        assert_eq!(out.tx_index, 0);
        assert_eq!(out.log_index, 0);
    }

    #[test]
    fn cursor_after_scan_limit_returns_none_at_query_end() {
        let out = cursor_after_scan_limit(20, 7, 8, 20);
        assert!(out.is_none());
    }
}
