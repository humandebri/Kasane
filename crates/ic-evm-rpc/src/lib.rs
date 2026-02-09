//! どこで: wrapperのRPC補助層 / 何を: eth系参照ロジックを分離 / なぜ: canister entrypointの責務を薄くするため

use evm_db::chain_data::{BlockData, ReceiptLike, StoredTx, StoredTxBytes, TxId, TxKind, TxLoc, TxLocKind};
use evm_db::stable_state::with_state;
use evm_db::types::keys::{make_account_key, make_code_key};
use evm_core::{chain, hash};
use ic_evm_rpc_types::{
    DecodedTxView, EthBlockView, EthLogFilterView, EthLogItemView, EthReceiptView, EthTxListView,
    EthTxView, GetLogsErrorView, LogView, RpcBlockLookupView, RpcReceiptLookupView, SubmitTxError,
    TxKindView,
};
use tracing::{error, warn};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TxApiErrorKind {
    InvalidArgument,
    Rejected,
}

const CODE_ARG_TX_TOO_LARGE: &str = "arg.tx_too_large";
const CODE_ARG_DECODE_FAILED: &str = "arg.decode_failed";
const CODE_ARG_UNSUPPORTED_TX_KIND: &str = "arg.unsupported_tx_kind";
const CODE_SUBMIT_TX_ALREADY_SEEN: &str = "submit.tx_already_seen";
const CODE_SUBMIT_INVALID_FEE: &str = "submit.invalid_fee";
const CODE_SUBMIT_NONCE_TOO_LOW: &str = "submit.nonce_too_low";
const CODE_SUBMIT_NONCE_GAP: &str = "submit.nonce_gap";
const CODE_SUBMIT_NONCE_CONFLICT: &str = "submit.nonce_conflict";
const CODE_SUBMIT_QUEUE_FULL: &str = "submit.queue_full";
const CODE_SUBMIT_SENDER_QUEUE_FULL: &str = "submit.sender_queue_full";
const CODE_SUBMIT_PRINCIPAL_QUEUE_FULL: &str = "submit.principal_queue_full";
const CODE_INTERNAL_UNEXPECTED: &str = "internal.unexpected";

pub fn rpc_eth_get_block_by_number_with_status(number: u64, full_tx: bool) -> RpcBlockLookupView {
    if let Some(pruned) = prune_boundary_for_number(number) {
        return RpcBlockLookupView::Pruned { pruned_before_block: pruned };
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

pub fn rpc_eth_get_transaction_receipt_by_eth_hash(eth_tx_hash: Vec<u8>) -> Option<EthReceiptView> {
    let tx_id = find_eth_tx_id_by_eth_hash_bytes(&eth_tx_hash)?;
    chain::get_receipt(&tx_id).map(receipt_to_eth_view)
}

pub fn rpc_eth_get_transaction_receipt_with_status(tx_hash: Vec<u8>) -> RpcReceiptLookupView {
    let Some(tx_id) = tx_id_from_bytes(tx_hash) else {
        return RpcReceiptLookupView::NotFound;
    };
    receipt_lookup_status(tx_id)
}

pub fn rpc_eth_get_balance(address: Vec<u8>) -> Result<Vec<u8>, String> {
    let addr = parse_address_20(address).ok_or_else(|| "address must be 20 bytes".to_string())?;
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
    let addr = parse_address_20(address).ok_or_else(|| "address must be 20 bytes".to_string())?;
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

pub fn rpc_eth_get_logs(filter: EthLogFilterView) -> Result<Vec<EthLogItemView>, GetLogsErrorView> {
    const DEFAULT_LIMIT: usize = 200;
    const MAX_LIMIT: usize = 2000;
    const MAX_BLOCK_SPAN: u64 = 5000;

    if filter.topic1.is_some() {
        return Err(GetLogsErrorView::UnsupportedFilter("topic1 is not supported".to_string()));
    }

    let head = chain::get_head_number();
    let mut from = filter.from_block.unwrap_or(0);
    let mut to = filter.to_block.unwrap_or(head);
    if from > to {
        return Err(GetLogsErrorView::InvalidArgument("from_block must be <= to_block".to_string()));
    }
    if to.saturating_sub(from) > MAX_BLOCK_SPAN {
        return Err(GetLogsErrorView::RangeTooLarge);
    }
    if to > head {
        to = head;
    }

    let requested_limit_u32 = filter.limit.unwrap_or(u32::try_from(DEFAULT_LIMIT).unwrap_or(u32::MAX));
    let requested_limit = usize::try_from(requested_limit_u32).unwrap_or(usize::MAX);
    if requested_limit > MAX_LIMIT {
        return Err(GetLogsErrorView::TooManyResults);
    }

    let address_filter = match filter.address {
        Some(bytes) => Some(parse_address_20(bytes).ok_or_else(|| GetLogsErrorView::InvalidArgument("address must be 20 bytes".to_string()))?),
        None => None,
    };
    let topic0_filter = match filter.topic0 {
        Some(bytes) => Some(parse_hash_32(bytes).ok_or_else(|| GetLogsErrorView::InvalidArgument("topic0 must be 32 bytes".to_string()))?),
        None => None,
    };

    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    if let Some(pruned) = pruned_before {
        if from <= pruned {
            from = pruned.saturating_add(1);
        }
    }

    let mut out = Vec::new();
    for number in from..=to {
        let Some(block) = chain::get_block(number) else { continue; };
        for tx_id in &block.tx_ids {
            let Some(receipt) = chain::get_receipt(tx_id) else { continue; };
            let eth_tx_hash = chain::get_tx_envelope(tx_id)
                .and_then(|envelope| StoredTx::try_from(envelope).ok())
                .and_then(|stored| if stored.kind == TxKind::EthSigned { Some(hash::keccak256(&stored.raw).to_vec()) } else { None });
            for (log_index, log) in receipt.logs.iter().enumerate() {
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
                    return Err(GetLogsErrorView::TooManyResults);
                }
                out.push(EthLogItemView {
                    block_number: receipt.block_number,
                    tx_index: receipt.tx_index,
                    log_index: u32::try_from(log_index).unwrap_or(u32::MAX),
                    tx_hash: receipt.tx_id.0.to_vec(),
                    eth_tx_hash: eth_tx_hash.clone(),
                    address: address.to_vec(),
                    topics: log.data.topics().iter().map(|topic| topic.as_slice().to_vec()).collect(),
                    data: log.data.data.to_vec(),
                });
            }
        }
    }
    Ok(out)
}

fn chain_submit_error_to_code(err: &chain::ChainError) -> Option<(TxApiErrorKind, &'static str)> {
    match err {
        chain::ChainError::TxTooLarge => Some((TxApiErrorKind::InvalidArgument, CODE_ARG_TX_TOO_LARGE)),
        chain::ChainError::DecodeFailed => Some((TxApiErrorKind::InvalidArgument, CODE_ARG_DECODE_FAILED)),
        chain::ChainError::UnsupportedTxKind => Some((TxApiErrorKind::InvalidArgument, CODE_ARG_UNSUPPORTED_TX_KIND)),
        chain::ChainError::TxAlreadySeen => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_TX_ALREADY_SEEN)),
        chain::ChainError::InvalidFee => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_INVALID_FEE)),
        chain::ChainError::NonceTooLow => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_NONCE_TOO_LOW)),
        chain::ChainError::NonceGap => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_NONCE_GAP)),
        chain::ChainError::NonceConflict => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_NONCE_CONFLICT)),
        chain::ChainError::QueueFull => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_QUEUE_FULL)),
        chain::ChainError::SenderQueueFull => Some((TxApiErrorKind::Rejected, CODE_SUBMIT_SENDER_QUEUE_FULL)),
        chain::ChainError::PrincipalQueueFull => {
            Some((TxApiErrorKind::Rejected, CODE_SUBMIT_PRINCIPAL_QUEUE_FULL))
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

fn parse_address_20(bytes: Vec<u8>) -> Option<[u8; 20]> {
    if bytes.len() != 20 {
        return None;
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&bytes);
    Some(out)
}

fn parse_hash_32(bytes: Vec<u8>) -> Option<[u8; 32]> {
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

fn find_eth_tx_id_by_eth_hash_bytes(eth_tx_hash: &[u8]) -> Option<TxId> {
    if eth_tx_hash.len() != 32 {
        return None;
    }
    with_state(|state| {
        for entry in state.tx_store.iter() {
            let tx_id = *entry.key();
            let Ok(stored) = StoredTx::try_from(entry.value()) else {
                continue;
            };
            if stored.kind != TxKind::EthSigned {
                continue;
            }
            if hash::keccak256(&stored.raw).as_slice() == eth_tx_hash {
                return Some(tx_id);
            }
        }
        None
    })
}

fn tx_to_view(tx_id: TxId) -> Option<EthTxView> {
    let envelope = chain::get_tx_envelope(&tx_id)?;
    let (block_number, tx_index) = match chain::get_tx_loc(&tx_id) {
        Some(TxLoc { kind: TxLocKind::Included, block_number, tx_index, .. }) => (Some(block_number), Some(tx_index)),
        _ => (None, None),
    };
    envelope_to_eth_view(envelope, block_number, tx_index)
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
    let decoded = if let Ok(decoded) = evm_core::tx_decode::decode_tx_view(kind, caller, &stored.raw) {
        Some(DecodedTxView {
            from: decoded.from.to_vec(),
            to: decoded.to.map(|addr| addr.to_vec()),
            nonce: decoded.nonce,
            value: decoded.value.to_vec(),
            input: decoded.input.into_owned(),
            gas_limit: decoded.gas_limit,
            gas_price: decoded.gas_price,
            chain_id: decoded.chain_id,
        })
    } else {
        None
    };

    Some(EthTxView {
        hash: stored.tx_id.0.to_vec(),
        eth_tx_hash: if kind == TxKind::EthSigned { Some(hash::keccak256(&stored.raw).to_vec()) } else { None },
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
        .and_then(|stored| if stored.kind == TxKind::EthSigned { Some(hash::keccak256(&stored.raw).to_vec()) } else { None });
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
        logs: receipt.logs.into_iter().map(log_to_view).collect(),
    }
}

fn log_to_view(log: evm_db::chain_data::receipt::LogEntry) -> LogView {
    LogView {
        address: log.address.as_slice().to_vec(),
        topics: log.data.topics().iter().map(|topic| topic.as_slice().to_vec()).collect(),
        data: log.data.data.to_vec(),
    }
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
        EthTxListView::Hashes(block.tx_ids.iter().map(|id| id.0.to_vec()).collect())
    };
    EthBlockView {
        number: block.number,
        parent_hash: block.parent_hash.to_vec(),
        block_hash: block.block_hash.to_vec(),
        timestamp: block.timestamp,
        txs,
        state_root: block.state_root.to_vec(),
    }
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
                    return RpcReceiptLookupView::Pruned { pruned_before_block: pruned };
                }
            }
        }
        return RpcReceiptLookupView::NotFound;
    }
    if let Some(pruned) = pruned_before {
        return RpcReceiptLookupView::PossiblyPruned { pruned_before_block: pruned };
    }
    RpcReceiptLookupView::NotFound
}
