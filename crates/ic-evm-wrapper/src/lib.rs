//! どこで: canister入口 / 何を: Phase1のAPI公開 / なぜ: submit中心の安全な運用導線を提供するため

use candid::{CandidType, Principal};
use evm_core::chain;
#[cfg(test)]
use evm_core::hash;
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::chain_data::constants::{MAX_QUEUE_SNAPSHOT_LIMIT, MAX_RETURN_DATA, MAX_TX_SIZE};
use evm_db::chain_data::{
    BlockData, CallerKey, MigrationPhase, OpsConfigV1, OpsMode, ReceiptLike, TxId, TxKind, TxLoc,
    TxLocKind, LOG_CONFIG_FILTER_MAX,
};
#[cfg(test)]
use evm_db::chain_data::StoredTx;
use evm_db::meta::{
    current_schema_version, ensure_meta_initialized, get_meta, mark_migration_applied,
    schema_migration_state, set_needs_migration, set_schema_migration_state, set_tx_locs_v3_active,
    SchemaMigrationPhase, SchemaMigrationState,
};
use evm_db::stable_state::{init_stable_state, with_state};
use evm_db::upgrade;
use ic_cdk::api::{
    accept_message, canister_cycle_balance, is_controller, msg_caller, msg_method_name, time,
};
use serde::Deserialize;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{error, info, warn};

#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Mutex, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use tracing_subscriber::fmt::MakeWriter;
#[cfg(not(target_arch = "wasm32"))]
use tracing_subscriber::EnvFilter;

#[cfg(feature = "canbench-rs")]
mod canbench_benches;

#[cfg(target_arch = "wasm32")]
getrandom::register_custom_getrandom!(always_fail_getrandom);

#[cfg(target_arch = "wasm32")]
fn always_fail_getrandom(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

const MAX_MINING_BACKOFF_MS: u64 = 300_000;

thread_local! {
    static MINING_FAIL_STREAK: Cell<u32> = Cell::new(0);
}

use ic_evm_rpc_types::*;


#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TxApiErrorKind {
    InvalidArgument,
    Rejected,
}

#[cfg(test)]
const CODE_ARG_TX_TOO_LARGE: &str = "arg.tx_too_large";
#[cfg(test)]
const CODE_ARG_DECODE_FAILED: &str = "arg.decode_failed";
#[cfg(test)]
const CODE_ARG_UNSUPPORTED_TX_KIND: &str = "arg.unsupported_tx_kind";
#[cfg(test)]
const CODE_SUBMIT_TX_ALREADY_SEEN: &str = "submit.tx_already_seen";
#[cfg(test)]
const CODE_SUBMIT_INVALID_FEE: &str = "submit.invalid_fee";
#[cfg(test)]
const CODE_SUBMIT_NONCE_TOO_LOW: &str = "submit.nonce_too_low";
#[cfg(test)]
const CODE_SUBMIT_NONCE_GAP: &str = "submit.nonce_gap";
#[cfg(test)]
const CODE_SUBMIT_NONCE_CONFLICT: &str = "submit.nonce_conflict";
#[cfg(test)]
const CODE_SUBMIT_QUEUE_FULL: &str = "submit.queue_full";
#[cfg(test)]
const CODE_SUBMIT_SENDER_QUEUE_FULL: &str = "submit.sender_queue_full";
#[cfg(test)]
const CODE_SUBMIT_PRINCIPAL_QUEUE_FULL: &str = "submit.principal_queue_full";
#[cfg(test)]
const CODE_INTERNAL_UNEXPECTED: &str = "internal.unexpected";

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub genesis_balances: Vec<GenesisBalanceView>,
}

impl InitArgs {
    fn validate(&self) -> Result<(), String> {
        let mut seen_addresses = std::collections::BTreeSet::new();
        if self.genesis_balances.is_empty() {
            return Err("genesis_balances must be non-empty".to_string());
        }
        for (idx, alloc) in self.genesis_balances.iter().enumerate() {
            if alloc.address.len() != 20 {
                return Err(format!("balance[{idx}].address must be 20 bytes"));
            }
            if alloc.amount == 0 {
                return Err(format!("balance[{idx}].amount must be > 0"));
            }
            if !seen_addresses.insert(alloc.address.clone()) {
                return Err(format!("duplicate genesis address at balance[{idx}]"));
            }
        }
        Ok(())
    }
}

#[cfg(not(feature = "canbench-rs"))]
#[ic_cdk::init]
fn init(args: Option<InitArgs>) {
    init_inner(args, true);
}

#[cfg(feature = "canbench-rs")]
#[ic_cdk::init]
fn init() {
    init_inner(None, false);
}

fn init_inner(args: Option<InitArgs>, require_args: bool) {
    init_stable_state();
    let _ = ensure_meta_initialized();
    init_tracing();
    let args = if require_args {
        args.unwrap_or_else(|| {
            ic_cdk::trap("InitArgsRequired: InitArgs is required; pass (opt record {...})")
        })
    } else {
        args.unwrap_or(InitArgs {
            genesis_balances: Vec::new(),
        })
    };
    if !args.genesis_balances.is_empty() {
        if let Err(reason) = args.validate() {
            ic_cdk::trap(&format!("InvalidInitArgs: {reason}"));
        }
        for alloc in args.genesis_balances.iter() {
            let mut addr = [0u8; 20];
            addr.copy_from_slice(&alloc.address);
            chain::dev_mint(addr, alloc.amount)
                .unwrap_or_else(|_| ic_cdk::trap("init: genesis mint failed"));
        }
    }
    observe_cycles();
    schedule_cycle_observer();
    schedule_prune();
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    upgrade::post_upgrade();
    init_stable_state();
    let _ = ensure_meta_initialized();
    init_tracing();
    apply_post_upgrade_migrations();
    observe_cycles();
    schedule_cycle_observer();
    schedule_prune();
}

#[ic_cdk::pre_upgrade]
fn pre_upgrade() {
    upgrade::pre_upgrade();
}

#[ic_cdk::inspect_message]
fn inspect_message() {
    let method = msg_method_name();
    let Some(limit) = inspect_payload_limit_for_method(method.as_str()) else {
        return;
    };
    if reject_anonymous_update().is_some() {
        return;
    }
    let payload_len = inspect_payload_len();
    if payload_len <= limit {
        accept_message();
    }
}

const INSPECT_TX_PAYLOAD_LIMIT: usize = MAX_TX_SIZE.saturating_mul(2);
const INSPECT_MANAGE_PAYLOAD_LIMIT: usize = MAX_TX_SIZE.saturating_mul(8);

#[derive(Clone, Copy)]
struct InspectMethodPolicy {
    method: &'static str,
    payload_limit: usize,
}

const INSPECT_METHOD_POLICIES: [InspectMethodPolicy; 14] = [
    InspectMethodPolicy {
        method: "submit_eth_tx",
        payload_limit: INSPECT_TX_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "submit_ic_tx",
        payload_limit: INSPECT_TX_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "rpc_eth_send_raw_transaction",
        payload_limit: INSPECT_TX_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_auto_mine",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_mining_interval_ms",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_block_gas_limit",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_instruction_soft_limit",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_prune_policy",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_pruning_enabled",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_ops_config",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_log_filter",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_miner_allowlist",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "prune_blocks",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "produce_block",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
];

fn inspect_payload_limit_for_method(method: &str) -> Option<usize> {
    inspect_policy_for_method(method).map(|policy| policy.payload_limit)
}

fn inspect_policy_for_method(method: &str) -> Option<InspectMethodPolicy> {
    if let Some(policy) = INSPECT_METHOD_POLICIES
        .iter()
        .copied()
        .find(|policy| policy.method == method)
    {
        return Some(policy);
    }
    if cfg!(feature = "dev-faucet") && method == "dev_mint" {
        return Some(InspectMethodPolicy {
            method: "dev_mint",
            payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
        });
    }
    None
}

#[allow(deprecated)]
fn inspect_payload_len() -> usize {
    ic_cdk::api::call::arg_data_raw_size()
}

#[cfg(test)]
fn submit_reject_code(err: &chain::ChainError) -> Option<&'static str> {
    match err {
        chain::ChainError::TxAlreadySeen => Some(CODE_SUBMIT_TX_ALREADY_SEEN),
        chain::ChainError::InvalidFee => Some(CODE_SUBMIT_INVALID_FEE),
        chain::ChainError::NonceTooLow => Some(CODE_SUBMIT_NONCE_TOO_LOW),
        chain::ChainError::NonceGap => Some(CODE_SUBMIT_NONCE_GAP),
        chain::ChainError::NonceConflict => Some(CODE_SUBMIT_NONCE_CONFLICT),
        chain::ChainError::QueueFull => Some(CODE_SUBMIT_QUEUE_FULL),
        chain::ChainError::SenderQueueFull => Some(CODE_SUBMIT_SENDER_QUEUE_FULL),
        chain::ChainError::PrincipalQueueFull => Some(CODE_SUBMIT_PRINCIPAL_QUEUE_FULL),
        _ => None,
    }
}

#[cfg(test)]
fn chain_submit_error_to_code(err: &chain::ChainError) -> Option<(TxApiErrorKind, &'static str)> {
    match err {
        chain::ChainError::TxTooLarge => {
            Some((TxApiErrorKind::InvalidArgument, CODE_ARG_TX_TOO_LARGE))
        }
        chain::ChainError::DecodeFailed => {
            Some((TxApiErrorKind::InvalidArgument, CODE_ARG_DECODE_FAILED))
        }
        chain::ChainError::UnsupportedTxKind => Some((
            TxApiErrorKind::InvalidArgument,
            CODE_ARG_UNSUPPORTED_TX_KIND,
        )),
        _ => submit_reject_code(err).map(|code| (TxApiErrorKind::Rejected, code)),
    }
}

#[cfg(test)]
fn map_submit_chain_error(err: chain::ChainError, op_name: &str) -> SubmitTxError {
    if let Some((kind, code)) = chain_submit_error_to_code(&err) {
        return match kind {
            TxApiErrorKind::InvalidArgument => SubmitTxError::InvalidArgument(code.to_string()),
            TxApiErrorKind::Rejected => SubmitTxError::Rejected(code.to_string()),
        };
    }
    error!(error = ?err, operation = op_name, "submit transaction failed");
    SubmitTxError::Internal(CODE_INTERNAL_UNEXPECTED.to_string())
}

#[cfg(test)]
fn chain_execute_error_to_code(err: &chain::ChainError) -> Option<(TxApiErrorKind, &'static str)> {
    match err {
        chain::ChainError::ExecFailed(exec) => {
            Some((TxApiErrorKind::Rejected, exec_error_to_code(exec.as_ref())))
        }
        _ => chain_submit_error_to_code(err),
    }
}

#[cfg(test)]
fn map_execute_chain_error(err: chain::ChainError) -> ExecuteTxError {
    if let Some((kind, code)) = chain_execute_error_to_code(&err) {
        return match kind {
            TxApiErrorKind::InvalidArgument => ExecuteTxError::InvalidArgument(code.to_string()),
            TxApiErrorKind::Rejected => ExecuteTxError::Rejected(code.to_string()),
        };
    }
    error!(error = ?err, "execute transaction failed");
    ExecuteTxError::Internal(CODE_INTERNAL_UNEXPECTED.to_string())
}

#[cfg(test)]
fn map_execute_chain_result(
    result: Result<chain::ExecResult, chain::ChainError>,
) -> Result<ExecResultDto, ExecuteTxError> {
    let result = match result {
        Ok(value) => value,
        Err(err) => return Err(map_execute_chain_error(err)),
    };
    Ok(ExecResultDto {
        tx_id: result.tx_id.0.to_vec(),
        block_number: result.block_number,
        tx_index: result.tx_index,
        status: result.status,
        gas_used: result.gas_used,
        return_data: clamp_return_data(result.return_data),
    })
}

#[ic_cdk::update]
fn submit_eth_tx(raw_tx: Vec<u8>) -> Result<Vec<u8>, SubmitTxError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(SubmitTxError::Rejected(reason));
    }
    if let Some(reason) = reject_write_reason() {
        return Err(SubmitTxError::Rejected(reason));
    }
    let out = ic_evm_rpc::submit_tx_in_with_code(
        chain::TxIn::EthSigned {
            tx_bytes: raw_tx,
            caller_principal: ic_cdk::api::msg_caller().as_slice().to_vec(),
        },
        "submit_eth_tx",
    );
    if out.is_ok() {
        schedule_mining();
    }
    out
}

#[ic_cdk::update]
fn submit_ic_tx(tx_bytes: Vec<u8>) -> Result<Vec<u8>, SubmitTxError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(SubmitTxError::Rejected(reason));
    }
    if let Some(reason) = reject_write_reason() {
        return Err(SubmitTxError::Rejected(reason));
    }
    let out = ic_evm_rpc::submit_tx_in_with_code(
        chain::TxIn::IcSynthetic {
            caller_principal: ic_cdk::api::msg_caller().as_slice().to_vec(),
            canister_id: ic_cdk::api::canister_self().as_slice().to_vec(),
            tx_bytes,
        },
        "submit_ic_tx",
    );
    if out.is_ok() {
        schedule_mining();
    }
    out
}

#[cfg(feature = "dev-faucet")]
#[ic_cdk::update]
fn dev_mint(address: Vec<u8>, amount: u128) {
    if let Some(reason) = reject_anonymous_update() {
        ic_cdk::trap(&reason);
    }
    if reject_write_reason().is_some() {
        return;
    }
    let caller = ic_cdk::api::msg_caller();
    if !ic_cdk::api::is_controller(&caller) {
        ic_cdk::trap("dev_mint: caller is not a controller");
    }
    if address.len() != 20 {
        ic_cdk::trap("dev_mint: address must be 20 bytes");
    }
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&address);
    chain::dev_mint(addr, amount).unwrap_or_else(|_| ic_cdk::trap("dev_mint failed"));
}

#[ic_cdk::update]
fn produce_block(max_txs: u32) -> Result<ProduceBlockStatus, ProduceBlockError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(ProduceBlockError::Internal(reason));
    }
    if let Err(reason) = require_producer_write() {
        return Err(ProduceBlockError::Internal(reason));
    }
    if migration_pending() {
        return Ok(ProduceBlockStatus::NoOp {
            reason: NoOpReason::NeedsMigration,
        });
    }
    if cycle_mode() == OpsMode::Critical {
        return Ok(ProduceBlockStatus::NoOp {
            reason: NoOpReason::CycleCritical,
        });
    }
    let limit = usize::try_from(max_txs).unwrap_or(0);
    match chain::produce_block(limit) {
        Ok(outcome) => {
            let block = outcome.block;
            info!(
                block_number = block.number,
                tx_count = block.tx_ids.len(),
                dropped = outcome.dropped,
                "produce_block succeeded"
            );
            Ok(ProduceBlockStatus::Produced {
                block_number: block.number,
                txs: block.tx_ids.len().try_into().unwrap_or(u32::MAX),
                gas_used: outcome.gas_used,
                dropped: outcome.dropped.try_into().unwrap_or(u32::MAX),
            })
        }
        Err(chain::ChainError::NoExecutableTx) | Err(chain::ChainError::QueueEmpty) => {
            Ok(ProduceBlockStatus::NoOp {
                reason: NoOpReason::NoExecutableTx,
            })
        }
        Err(chain::ChainError::InvalidLimit) => Err(ProduceBlockError::InvalidArgument(
            "max_txs must be > 0".to_string(),
        )),
        Err(err) => {
            error!(error = ?err, "produce_block failed");
            Err(ProduceBlockError::Internal("internal error".to_string()))
        }
    }
}

#[ic_cdk::query]
fn get_block(number: u64) -> Result<BlockView, LookupError> {
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    if let Some(pruned) = pruned_before {
        if number <= pruned {
            return Err(LookupError::Pruned {
                pruned_before_block: pruned,
            });
        }
    }
    chain::get_block(number)
        .map(block_to_view)
        .ok_or(LookupError::NotFound)
}

#[ic_cdk::query]
fn get_receipt(tx_id: Vec<u8>) -> Result<ReceiptView, LookupError> {
    if tx_id.len() != 32 {
        return Err(LookupError::NotFound);
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    if let Some(receipt) = chain::get_receipt(&TxId(buf)) {
        return Ok(receipt_to_view(receipt));
    }
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    let loc = chain::get_tx_loc(&TxId(buf));
    if let Some(loc) = loc {
        if loc.kind == TxLocKind::Queued {
            return Err(LookupError::Pending);
        }
        if loc.kind == TxLocKind::Included {
            if let Some(pruned) = pruned_before {
                if loc.block_number <= pruned {
                    return Err(LookupError::Pruned {
                        pruned_before_block: pruned,
                    });
                }
            }
        }
    }
    Err(LookupError::NotFound)
}

#[ic_cdk::query]
fn export_blocks(
    cursor: Option<ExportCursorView>,
    max_bytes: u32,
) -> Result<ExportResponseView, ExportErrorView> {
    let core_cursor = cursor.map(|value| evm_core::export::ExportCursor {
        block_number: value.block_number,
        segment: value.segment,
        byte_offset: value.byte_offset,
    });
    let result =
        evm_core::export::export_blocks(core_cursor, max_bytes).map_err(export_error_to_view)?;
    Ok(ExportResponseView {
        chunks: result
            .chunks
            .into_iter()
            .map(|chunk| ExportChunkView {
                segment: chunk.segment,
                start: chunk.start,
                bytes: chunk.bytes,
                payload_len: chunk.payload_len,
            })
            .collect(),
        next_cursor: result.next_cursor.map(|value| ExportCursorView {
            block_number: value.block_number,
            segment: value.segment,
            byte_offset: value.byte_offset,
        }),
    })
}

fn export_error_to_view(err: evm_core::export::ExportError) -> ExportErrorView {
    match err {
        evm_core::export::ExportError::InvalidCursor(message) => ExportErrorView::InvalidCursor {
            message: message.to_string(),
        },
        evm_core::export::ExportError::Pruned {
            pruned_before_block,
        } => ExportErrorView::Pruned {
            pruned_before_block,
        },
        evm_core::export::ExportError::MissingData(message) => ExportErrorView::MissingData {
            message: message.to_string(),
        },
        evm_core::export::ExportError::Limit => ExportErrorView::Limit,
    }
}

#[ic_cdk::query]
fn rpc_eth_chain_id() -> u64 {
    CHAIN_ID
}

#[ic_cdk::query]
fn rpc_eth_block_number() -> u64 {
    chain::get_head_number()
}

#[ic_cdk::update]
fn set_prune_policy(policy: PrunePolicyView) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    let core_policy = evm_db::chain_data::PrunePolicy {
        target_bytes: policy.target_bytes,
        retain_days: policy.retain_days,
        retain_blocks: policy.retain_blocks,
        headroom_ratio_bps: policy.headroom_ratio_bps,
        hard_emergency_ratio_bps: policy.hard_emergency_ratio_bps,
        timer_interval_ms: policy.timer_interval_ms,
        max_ops_per_tick: policy.max_ops_per_tick,
    };
    chain::set_prune_policy(core_policy).map_err(|_| "set_prune_policy failed".to_string())?;
    schedule_prune();
    Ok(())
}

#[ic_cdk::update]
fn set_pruning_enabled(enabled: bool) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    chain::set_pruning_enabled(enabled).map_err(|_| "set_pruning_enabled failed".to_string())?;
    schedule_prune();
    Ok(())
}

#[ic_cdk::query]
fn get_prune_status() -> PruneStatusView {
    let status = chain::get_prune_status();
    PruneStatusView {
        pruning_enabled: status.pruning_enabled,
        prune_running: status.prune_running,
        estimated_kept_bytes: status.estimated_kept_bytes,
        high_water_bytes: status.high_water_bytes,
        low_water_bytes: status.low_water_bytes,
        hard_emergency_bytes: status.hard_emergency_bytes,
        last_prune_at: status.last_prune_at,
        pruned_before_block: status.pruned_before_block,
        oldest_kept_block: status.oldest_kept_block,
        oldest_kept_timestamp: status.oldest_kept_timestamp,
        need_prune: status.need_prune,
    }
}

#[ic_cdk::query]
fn rpc_eth_get_block_by_number(number: u64, full_tx: bool) -> Option<EthBlockView> {
    match rpc_eth_get_block_by_number_with_status(number, full_tx) {
        RpcBlockLookupView::Found(block) => Some(block),
        RpcBlockLookupView::Pruned { .. } | RpcBlockLookupView::NotFound => None,
    }
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_by_eth_hash(eth_tx_hash: Vec<u8>) -> Option<EthTxView> {
    ic_evm_rpc::rpc_eth_get_transaction_by_eth_hash(eth_tx_hash)
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_receipt_by_eth_hash(eth_tx_hash: Vec<u8>) -> Option<EthReceiptView> {
    ic_evm_rpc::rpc_eth_get_transaction_receipt_by_eth_hash(eth_tx_hash)
}

#[ic_cdk::query]
fn rpc_eth_get_balance(address: Vec<u8>) -> Result<Vec<u8>, String> {
    ic_evm_rpc::rpc_eth_get_balance(address)
}

#[ic_cdk::query]
fn rpc_eth_get_code(address: Vec<u8>) -> Result<Vec<u8>, String> {
    ic_evm_rpc::rpc_eth_get_code(address)
}

#[ic_cdk::query]
fn rpc_eth_call_rawtx(raw_tx: Vec<u8>) -> Result<Vec<u8>, String> {
    ic_evm_rpc::rpc_eth_call_rawtx(raw_tx)
}

#[ic_cdk::query]
fn rpc_eth_get_logs(filter: EthLogFilterView) -> Result<Vec<EthLogItemView>, GetLogsErrorView> {
    ic_evm_rpc::rpc_eth_get_logs(filter)
}

#[ic_cdk::query]
fn rpc_eth_get_block_by_number_with_status(number: u64, full_tx: bool) -> RpcBlockLookupView {
    ic_evm_rpc::rpc_eth_get_block_by_number_with_status(number, full_tx)
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_receipt_with_status(tx_hash: Vec<u8>) -> RpcReceiptLookupView {
    ic_evm_rpc::rpc_eth_get_transaction_receipt_with_status(tx_hash)
}

#[ic_cdk::update]
fn rpc_eth_send_raw_transaction(raw_tx: Vec<u8>) -> Result<Vec<u8>, SubmitTxError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(SubmitTxError::Rejected(reason));
    }
    if critical_corrupt_state() {
        return Err(SubmitTxError::Rejected(
            "rpc.state_unavailable.corrupt_or_migrating".to_string(),
        ));
    }
    if let Some(reason) = reject_write_reason() {
        return Err(SubmitTxError::Rejected(reason));
    }
    let out = ic_evm_rpc::rpc_eth_send_raw_transaction(
        raw_tx,
        ic_cdk::api::msg_caller().as_slice().to_vec(),
    );
    if out.is_ok() {
        schedule_mining();
    }
    out
}

#[ic_cdk::query]
fn get_miner_allowlist() -> Vec<Principal> {
    with_state(|state| {
        let mut out = Vec::new();
        for entry in state.miner_allowlist.iter() {
            if let Some(principal) = caller_key_to_principal(*entry.key()) {
                out.push(principal);
            }
        }
        out
    })
}

#[ic_cdk::update]
fn set_miner_allowlist(principals: Vec<Principal>) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    evm_db::stable_state::with_state_mut(|state| {
        let old_keys: Vec<_> = state
            .miner_allowlist
            .iter()
            .map(|entry| *entry.key())
            .collect();
        for key in old_keys {
            state.miner_allowlist.remove(&key);
        }
        for principal in principals {
            let key = caller_key_from_principal(principal);
            state.miner_allowlist.insert(key, 1u8);
        }
    });
    Ok(())
}

#[ic_cdk::query]
fn get_cycle_balance() -> u128 {
    canister_cycle_balance()
}

#[ic_cdk::query]
fn get_ops_status() -> OpsStatusView {
    with_state(|state| {
        let config = *state.ops_config.get();
        let ops = *state.ops_state.get();
        let meta = get_meta();
        let decode_failure_last_label =
            ic_evm_ops::decode_failure_label_view(evm_db::corrupt_log::read_last_corrupt_tag());
        OpsStatusView {
            config: OpsConfigView {
                low_watermark: config.low_watermark,
                critical: config.critical,
                freeze_on_critical: config.freeze_on_critical,
            },
            last_cycle_balance: ops.last_cycle_balance,
            last_check_ts: ops.last_check_ts,
            mode: ic_evm_ops::mode_to_view(ops.mode),
            safe_stop_latched: ops.safe_stop_latched,
            needs_migration: meta.needs_migration,
            schema_version: meta.schema_version,
            log_filter_override: state.log_config.get().filter().map(str::to_string),
            log_truncated_count: LOG_TRUNCATED_COUNT.load(Ordering::Relaxed),
            critical_corrupt: critical_corrupt_state(),
            mining_error_count: MINING_ERROR_COUNT.load(Ordering::Relaxed),
            prune_error_count: PRUNE_ERROR_COUNT.load(Ordering::Relaxed),
            decode_failure_count: evm_db::corrupt_log::read_corrupt_count(),
            decode_failure_last_ts: evm_db::corrupt_log::read_last_corrupt_ts(),
            decode_failure_last_label,
            block_gas_limit: state.chain_state.get().block_gas_limit,
            instruction_soft_limit: state.chain_state.get().instruction_soft_limit,
        }
    })
}

#[cfg(test)]
fn decode_failure_label_view(raw: [u8; 32]) -> Option<String> {
    ic_evm_ops::decode_failure_label_view(raw)
}

#[ic_cdk::update]
fn set_ops_config(config: OpsConfigView) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    if config.critical == 0 {
        return Err("input.ops_config.critical.non_positive".to_string());
    }
    if config.critical >= config.low_watermark {
        return Err("input.ops_config.critical.gte_low_watermark".to_string());
    }
    evm_db::stable_state::with_state_mut(|state| {
        let _ = state.ops_config.set(OpsConfigV1 {
            low_watermark: config.low_watermark,
            critical: config.critical,
            freeze_on_critical: config.freeze_on_critical,
        });
    });
    observe_cycles();
    Ok(())
}

#[ic_cdk::update]
fn set_log_filter(filter: Option<String>) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    let normalized = filter
        .map(|raw| raw.trim().to_string())
        .filter(|v| !v.is_empty());
    if let Some(value) = normalized.as_ref() {
        if value.len() > LOG_CONFIG_FILTER_MAX {
            return Err("input.log_filter.too_long".to_string());
        }
    }
    evm_db::stable_state::with_state_mut(|state| {
        let _ = state.log_config.set(evm_db::chain_data::LogConfigV1 {
            has_filter: normalized.is_some(),
            filter: normalized.unwrap_or_default(),
        });
    });
    Ok(())
}

#[ic_cdk::query]
fn expected_nonce_by_address(address: Vec<u8>) -> Result<u64, String> {
    if address.len() != 20 {
        return Err("address must be 20 bytes".to_string());
    }
    let mut buf = [0u8; 20];
    buf.copy_from_slice(&address);
    Ok(chain::expected_nonce_for_sender_view(buf))
}

#[ic_cdk::query]
fn health() -> HealthView {
    with_state(|state| {
        let head = *state.head.get();
        let chain_state = *state.chain_state.get();
        let queue_len = state.pending_by_sender_nonce.len();
        HealthView {
            tip_number: head.number,
            tip_hash: head.block_hash.to_vec(),
            last_block_time: chain_state.last_block_time,
            queue_len,
            auto_mine_enabled: chain_state.auto_mine_enabled,
            is_producing: chain_state.is_producing,
            mining_scheduled: chain_state.mining_scheduled,
            block_gas_limit: chain_state.block_gas_limit,
            instruction_soft_limit: chain_state.instruction_soft_limit,
        }
    })
}

#[ic_cdk::query]
fn metrics(window: u64) -> MetricsView {
    let cycles = ic_cdk::api::canister_cycle_balance();
    with_state(|state| {
        let queue_len = state.pending_by_sender_nonce.len();
        let window = clamp_metrics_window(window);
        let metrics = *state.metrics_state.get();
        let summary = metrics.window_summary(window);
        let pruned_before_block = state.prune_state.get().pruned_before();
        let rate = summary.block_rate_per_sec_x1000();
        let avg = if summary.blocks == 0 {
            0
        } else {
            summary.txs / summary.blocks
        };
        MetricsView {
            window,
            blocks: summary.blocks,
            txs: summary.txs,
            avg_txs_per_block: avg,
            block_rate_per_sec_x1000: rate,
            ema_block_rate_per_sec_x1000: metrics.ema_block_rate_x1000,
            ema_txs_per_block_x1000: metrics.ema_txs_per_block_x1000,
            queue_len,
            drop_counts: collect_drop_counts(&metrics),
            total_submitted: metrics.total_submitted,
            total_included: metrics.total_included,
            total_dropped: metrics.total_dropped,
            cycles,
            pruned_before_block,
            dev_faucet_enabled: cfg!(feature = "dev-faucet"),
        }
    })
}

#[ic_cdk::query]
fn metrics_prometheus() -> Result<String, String> {
    let cycles = canister_cycle_balance();
    let stable_memory_pages = ic_cdk::stable::stable_size();
    let heap_memory_pages = current_heap_memory_pages();
    let now_nanos = time();
    let snapshot = with_state(|state| {
        let head = *state.head.get();
        let chain_state = *state.chain_state.get();
        let metrics = *state.metrics_state.get();
        let pruned_before_block = state.prune_state.get().pruned_before();
        ic_evm_metrics::build_prometheus_snapshot(ic_evm_metrics::PrometheusSnapshotInput {
            cycles_balance: cycles,
            stable_memory_pages,
            heap_memory_pages,
            tip_block_number: head.number,
            queue_len: state.pending_by_sender_nonce.len(),
            total_submitted: metrics.total_submitted,
            total_included: metrics.total_included,
            total_dropped: metrics.total_dropped,
            auto_mine_enabled: chain_state.auto_mine_enabled,
            is_producing: chain_state.is_producing,
            mining_scheduled: chain_state.mining_scheduled,
            mining_interval_ms: chain_state.mining_interval_ms,
            last_block_time: chain_state.last_block_time,
            pruned_before_block,
            drop_counts_by_code: metrics.drop_counts.to_vec(),
        })
    });
    ic_evm_metrics::encode_prometheus(now_nanos, &snapshot)
}

#[ic_cdk::update]
fn set_auto_mine(enabled: bool) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.auto_mine_enabled = enabled;
        state.chain_state.set(chain_state);
    });
    if enabled {
        schedule_mining();
    }
    Ok(())
}

#[ic_cdk::update]
fn set_mining_interval_ms(interval_ms: u64) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    if interval_ms == 0 {
        return Err("input.mining_interval.non_positive".to_string());
    }
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.mining_interval_ms = interval_ms;
        state.chain_state.set(chain_state);
    });
    schedule_mining();
    Ok(())
}

#[ic_cdk::update]
fn set_block_gas_limit(limit: u64) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    if limit == 0 {
        return Err("input.block_gas_limit.non_positive".to_string());
    }
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.block_gas_limit = limit;
        state.chain_state.set(chain_state);
    });
    Ok(())
}

#[ic_cdk::update]
fn set_instruction_soft_limit(limit: u64) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_manage_write()?;
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.instruction_soft_limit = limit;
        state.chain_state.set(chain_state);
    });
    Ok(())
}

#[ic_cdk::update]
fn prune_blocks(retain: u64, max_ops: u32) -> Result<PruneResultView, ProduceBlockError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(ProduceBlockError::Internal(reason));
    }
    if let Err(reason) = require_manage_write() {
        return Err(ProduceBlockError::Internal(reason));
    }
    match chain::prune_blocks(retain, max_ops) {
        Ok(result) => Ok(PruneResultView {
            did_work: result.did_work,
            remaining: result.remaining,
            pruned_before_block: result.pruned_before_block,
        }),
        Err(chain::ChainError::InvalidLimit) => Err(ProduceBlockError::InvalidArgument(
            "retain/max_ops must be > 0".to_string(),
        )),
        Err(_err) => Err(ProduceBlockError::Internal("internal error".to_string())),
    }
}

#[ic_cdk::query]
fn get_pending(tx_id: Vec<u8>) -> PendingStatusView {
    if tx_id.len() != 32 {
        return PendingStatusView::Unknown;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    let loc = chain::get_tx_loc(&TxId(buf));
    pending_to_view(loc)
}

fn block_to_view(block: BlockData) -> BlockView {
    let mut tx_ids = Vec::with_capacity(block.tx_ids.len());
    for tx_id in block.tx_ids.into_iter() {
        tx_ids.push(tx_id.0.to_vec());
    }
    BlockView {
        number: block.number,
        parent_hash: block.parent_hash.to_vec(),
        block_hash: block.block_hash.to_vec(),
        timestamp: block.timestamp,
        tx_ids,
        tx_list_hash: block.tx_list_hash.to_vec(),
        state_root: block.state_root.to_vec(),
    }
}

fn clamp_return_data(return_data: Vec<u8>) -> Option<Vec<u8>> {
    if return_data.len() > MAX_RETURN_DATA {
        return None;
    }
    Some(return_data)
}

fn receipt_to_view(receipt: ReceiptLike) -> ReceiptView {
    ReceiptView {
        tx_id: receipt.tx_id.0.to_vec(),
        block_number: receipt.block_number,
        tx_index: receipt.tx_index,
        status: receipt.status,
        gas_used: receipt.gas_used,
        effective_gas_price: receipt.effective_gas_price,
        l1_data_fee: receipt.l1_data_fee,
        operator_fee: receipt.operator_fee,
        total_fee: receipt.total_fee,
        return_data_hash: receipt.return_data_hash.to_vec(),
        return_data: clamp_return_data(receipt.return_data),
        contract_address: receipt.contract_address.map(|v| v.to_vec()),
        logs: receipt.logs.into_iter().map(log_to_view).collect(),
    }
}

fn log_to_view(log: evm_db::chain_data::receipt::LogEntry) -> LogView {
    LogView {
        address: log.address.as_slice().to_vec(),
        topics: log
            .data
            .topics()
            .iter()
            .map(|topic| topic.as_slice().to_vec())
            .collect(),
        data: log.data.data.to_vec(),
    }
}

fn tx_kind_to_view(kind: TxKind) -> TxKindView {
    match kind {
        TxKind::EthSigned => TxKindView::EthSigned,
        TxKind::IcSynthetic => TxKindView::IcSynthetic,
    }
}

#[cfg(test)]
fn exec_error_to_code(err: Option<&evm_core::revm_exec::ExecError>) -> &'static str {
    use evm_core::revm_exec::{ExecError, OpHaltReason, OpTransactionError};

    match err {
        None => "exec.execution.failed",
        Some(ExecError::Decode(_)) => "exec.decode.failed",
        Some(ExecError::TxError(OpTransactionError::TxBuildFailed)) => "exec.tx.build_failed",
        Some(ExecError::TxError(OpTransactionError::TxRejectedByPolicy)) => {
            "exec.tx.rejected_by_policy"
        }
        Some(ExecError::TxError(OpTransactionError::TxPrecheckFailed)) => "exec.tx.precheck_failed",
        Some(ExecError::TxError(OpTransactionError::TxExecutionFailed)) => {
            "exec.tx.execution_failed"
        }
        Some(ExecError::Revert) => "exec.revert",
        Some(ExecError::EvmHalt(OpHaltReason::OutOfGas)) => "exec.halt.out_of_gas",
        Some(ExecError::EvmHalt(OpHaltReason::InvalidOpcode)) => "exec.halt.invalid_opcode",
        Some(ExecError::EvmHalt(OpHaltReason::StackOverflow)) => "exec.halt.stack_overflow",
        Some(ExecError::EvmHalt(OpHaltReason::StackUnderflow)) => "exec.halt.stack_underflow",
        Some(ExecError::EvmHalt(OpHaltReason::InvalidJump)) => "exec.halt.invalid_jump",
        Some(ExecError::EvmHalt(OpHaltReason::StateChangeDuringStaticCall)) => {
            "exec.halt.static_state_change"
        }
        Some(ExecError::EvmHalt(OpHaltReason::PrecompileError)) => "exec.halt.precompile_error",
        Some(ExecError::EvmHalt(OpHaltReason::Unknown)) => "exec.halt.unknown",
        Some(ExecError::InvalidGasFee) => "exec.gas_fee.invalid",
        Some(ExecError::ResultTooLarge) => "exec.result.too_large",
        Some(ExecError::ExecutionFailed) => "exec.execution.failed",
    }
}

fn pending_to_view(loc: Option<TxLoc>) -> PendingStatusView {
    match loc {
        Some(TxLoc {
            kind: TxLocKind::Queued,
            seq,
            ..
        }) => PendingStatusView::Queued { seq },
        Some(TxLoc {
            kind: TxLocKind::Included,
            block_number,
            tx_index,
            ..
        }) => PendingStatusView::Included {
            block_number,
            tx_index,
        },
        Some(TxLoc {
            kind: TxLocKind::Dropped,
            drop_code,
            ..
        }) => PendingStatusView::Dropped { code: drop_code },
        None => PendingStatusView::Unknown,
    }
}

fn clamp_metrics_window(window: u64) -> u64 {
    const DEFAULT_WINDOW: u64 = 128;
    const MAX_WINDOW: u64 = 2048;
    if window == 0 {
        return DEFAULT_WINDOW;
    }
    if window > MAX_WINDOW {
        return MAX_WINDOW;
    }
    window
}

fn collect_drop_counts(metrics: &evm_db::chain_data::MetricsStateV1) -> Vec<DropCountView> {
    metrics
        .drop_counts
        .iter()
        .enumerate()
        .filter_map(|(idx, count)| {
            if *count == 0 {
                None
            } else {
                Some(DropCountView {
                    code: idx as u16,
                    count: *count,
                })
            }
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
fn current_heap_memory_pages() -> u64 {
    u64::try_from(core::arch::wasm32::memory_size(0)).unwrap_or(u64::MAX)
}

#[cfg(not(target_arch = "wasm32"))]
fn current_heap_memory_pages() -> u64 {
    0
}

#[cfg(test)]
fn tx_id_from_bytes(tx_id: Vec<u8>) -> Option<TxId> {
    if tx_id.len() != 32 {
        return None;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    Some(TxId(buf))
}

#[cfg(test)]
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


#[cfg(test)]
fn prune_boundary_for_number(number: u64) -> Option<u64> {
    let pruned_before = with_state(|state| state.prune_state.get().pruned_before());
    match pruned_before {
        Some(pruned) if number <= pruned => Some(pruned),
        _ => None,
    }
}

#[cfg(test)]
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

fn require_controller() -> Result<(), String> {
    let caller = msg_caller();
    if !is_controller(&caller) {
        return Err("auth.controller_required".to_string());
    }
    Ok(())
}

fn require_manage_write() -> Result<(), String> {
    require_controller()?;
    if let Some(reason) = reject_write_reason() {
        return Err(reason);
    }
    Ok(())
}

fn require_producer_write() -> Result<(), String> {
    if let Some(reason) = reject_write_reason() {
        return Err(reason);
    }
    let caller = msg_caller();
    if is_controller(&caller) {
        return Ok(());
    }
    let key = CallerKey::from_principal_bytes(caller.as_slice());
    let allowed = with_state(|state| state.miner_allowlist.get(&key).is_some());
    if !allowed {
        return Err("auth.producer_required".to_string());
    }
    Ok(())
}

fn caller_key_from_principal(principal: Principal) -> CallerKey {
    CallerKey::from_principal_bytes(principal.as_slice())
}

fn caller_key_to_principal(key: CallerKey) -> Option<Principal> {
    let len = usize::from(key.0[0]);
    if len == 0 || len > 29 {
        return None;
    }
    Some(Principal::from_slice(&key.0[1..1 + len]))
}

fn migration_pending() -> bool {
    let meta = get_meta();
    if meta.needs_migration || meta.schema_version < current_schema_version() {
        return true;
    }
    with_state(|state| {
        !state.state_root_meta.get().initialized
            || state.state_root_migration.get().phase != MigrationPhase::Done
    })
}

fn drive_migrations_tick(schema_max_steps: u32, state_root_max_steps: u32) {
    let _ = schema_migration_tick(schema_max_steps);
    let _ = chain::state_root_migration_tick(state_root_max_steps);
}

fn critical_corrupt_state() -> bool {
    let meta = get_meta();
    if meta.needs_migration {
        return true;
    }
    matches!(schema_migration_state().phase, SchemaMigrationPhase::Error)
}

fn schema_migration_tick(max_steps: u32) -> bool {
    let mut steps = 0u32;
    while steps < max_steps {
        let mut state = schema_migration_state();
        match state.phase {
            SchemaMigrationPhase::Done => return true,
            SchemaMigrationPhase::Error => return false,
            SchemaMigrationPhase::Init => {
                if state.from_version < 3 {
                    set_tx_locs_v3_active(false);
                    chain::clear_tx_locs_v3();
                    state.cursor_key_set = false;
                    state.cursor_key = [0u8; 32];
                }
                state.phase = SchemaMigrationPhase::Scan;
                state.cursor = 0;
                set_schema_migration_state(state);
            }
            SchemaMigrationPhase::Scan => {
                state.phase = SchemaMigrationPhase::Rewrite;
                state.cursor = 0;
                set_schema_migration_state(state);
            }
            SchemaMigrationPhase::Rewrite => {
                if state.from_version < 3 {
                    let start_key = if state.cursor_key_set {
                        Some(TxId(state.cursor_key))
                    } else {
                        None
                    };
                    let (last_key, copied, done) = chain::migrate_tx_locs_batch(start_key, 512);
                    state.cursor = state.cursor.saturating_add(copied);
                    if let Some(key) = last_key {
                        state.cursor_key_set = true;
                        state.cursor_key = key.0;
                    }
                    set_schema_migration_state(state);
                    if !done {
                        return false;
                    }
                }
                if state.from_version < 4 {
                    chain::rebuild_pending_runtime_indexes();
                }
                state.phase = SchemaMigrationPhase::Verify;
                state.cursor = 0;
                set_schema_migration_state(state);
            }
            SchemaMigrationPhase::Verify => {
                if state.from_version < 3 {
                    let tx_locs_migrated = with_state(|s| s.tx_locs.len() == s.tx_locs_v3.len());
                    if !tx_locs_migrated {
                        state.phase = SchemaMigrationPhase::Error;
                        state.last_error = 1;
                        set_schema_migration_state(state);
                        return false;
                    }
                    set_tx_locs_v3_active(true);
                } else if !evm_db::meta::tx_locs_v3_active() {
                    state.phase = SchemaMigrationPhase::Error;
                    state.last_error = 2;
                    set_schema_migration_state(state);
                    return false;
                }
                if state.from_version < 4 {
                    let indexes_ok = with_state(|s| {
                        let pending_len = s.pending_by_sender_nonce.len();
                        let fee_idx_len = s.pending_fee_key_by_tx_id.len();
                        let ready_len = s.ready_key_by_tx_id.len();
                        let ready_seq_len = s.ready_by_seq.len();
                        let mut principal_total = 0u64;
                        for entry in s.principal_pending_count.iter() {
                            principal_total =
                                principal_total.saturating_add(u64::from(entry.value()));
                        }
                        pending_len == fee_idx_len
                            && ready_len == ready_seq_len
                            && principal_total == pending_len
                    });
                    if !indexes_ok {
                        state.phase = SchemaMigrationPhase::Error;
                        state.last_error = 3;
                        set_schema_migration_state(state);
                        return false;
                    }
                }
                if state.from_version < 2 {
                    chain::clear_mempool_on_upgrade();
                }
                mark_migration_applied(state.from_version, state.to_version, time());
                set_needs_migration(false);
                state.phase = SchemaMigrationPhase::Done;
                state.cursor = 0;
                set_schema_migration_state(state);
                return true;
            }
        }
        steps = steps.saturating_add(1);
    }
    false
}

fn observe_cycles() -> OpsMode {
    let balance = canister_cycle_balance();
    let now = time();
    evm_db::stable_state::with_state_mut(|state| {
        let config = *state.ops_config.get();
        let ops = ic_evm_ops::observe_cycles(balance, now, config, *state.ops_state.get());
        let next_mode = ops.mode;
        let _ = state.ops_state.set(ops);
        next_mode
    })
}

fn cycle_mode() -> OpsMode {
    observe_cycles()
}

fn reject_write_reason() -> Option<String> {
    ic_evm_ops::reject_write_reason_with_mode_provider(migration_pending(), cycle_mode)
}

fn reject_anonymous_update() -> Option<String> {
    reject_anonymous_principal(msg_caller())
}

fn reject_anonymous_principal(caller: Principal) -> Option<String> {
    if caller == Principal::anonymous() {
        return Some("auth.anonymous_forbidden".to_string());
    }
    None
}

#[cfg(not(target_arch = "wasm32"))]
fn init_tracing() {
    static LOG_INIT: OnceLock<()> = OnceLock::new();
    let _ = LOG_INIT.get_or_init(|| {
        let env_filter = EnvFilter::new(resolve_log_filter().unwrap_or_else(|| "warn".to_string()));
        let _ = tracing_subscriber::fmt()
            .json()
            .with_target(true)
            .with_current_span(false)
            .with_span_list(false)
            .with_writer(IcDebugPrintMakeWriter)
            .with_env_filter(env_filter)
            .try_init();
    });
}

#[cfg(target_arch = "wasm32")]
fn init_tracing() {}

#[cfg(not(target_arch = "wasm32"))]
fn resolve_log_filter() -> Option<String> {
    if let Some(value) = read_env_var_guarded("LOG_FILTER", LOG_FILTER_MAX_LEN) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    with_state(|state| state.log_config.get().filter().map(str::to_string))
}

#[cfg(all(not(feature = "canbench-rs"), not(target_arch = "wasm32")))]
const MAX_ENV_VAR_NAME_LEN: usize = 128;
#[cfg(not(target_arch = "wasm32"))]
const LOG_FILTER_MAX_LEN: usize = 256;
static LOG_TRUNCATED_COUNT: AtomicU64 = AtomicU64::new(0);
static MINING_ERROR_COUNT: AtomicU64 = AtomicU64::new(0);
static PRUNE_ERROR_COUNT: AtomicU64 = AtomicU64::new(0);

#[cfg(all(not(feature = "canbench-rs"), not(target_arch = "wasm32")))]
fn read_env_var_guarded(name: &str, max_value_len: usize) -> Option<String> {
    if name.len() > MAX_ENV_VAR_NAME_LEN {
        return None;
    }
    if !ic_cdk::api::env_var_name_exists(name) {
        return None;
    }
    let value = ic_cdk::api::env_var_value(name);
    if value.len() > max_value_len {
        warn!(
            env_var = name,
            max_value_len,
            actual_len = value.len(),
            "env var value too long; ignored"
        );
        return None;
    }
    Some(value)
}

#[cfg(feature = "canbench-rs")]
#[allow(dead_code)]
fn read_env_var_guarded(_name: &str, _max_value_len: usize) -> Option<String> {
    None
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Copy)]
struct IcDebugPrintMakeWriter;

#[cfg(not(target_arch = "wasm32"))]
impl<'a> MakeWriter<'a> for IcDebugPrintMakeWriter {
    type Writer = IcDebugPrintWriter;

    fn make_writer(&'a self) -> Self::Writer {
        IcDebugPrintWriter {
            buffer: String::new(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct IcDebugPrintWriter {
    buffer: String,
}

#[cfg(not(target_arch = "wasm32"))]
impl Write for IcDebugPrintWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.buffer.push_str(&String::from_utf8_lossy(buf));
        emit_complete_lines(&mut self.buffer);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.buffer.trim().is_empty() {
            emit_bounded_log_line(self.buffer.trim());
            self.buffer.clear();
        }
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for IcDebugPrintWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn emit_complete_lines(buffer: &mut String) {
    static REENTRANT_GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = REENTRANT_GUARD.get_or_init(|| Mutex::new(())).lock();
    if guard.is_err() {
        if !buffer.is_empty() {
            emit_debug_print("{\"target\":\"tracing\",\"fallback\":true}".to_string());
        }
        return;
    }
    while let Some(newline_index) = buffer.find('\n') {
        let line: String = buffer.drain(..=newline_index).collect();
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            emit_bounded_log_line(trimmed);
        }
    }
}

#[cfg(all(not(target_arch = "wasm32"), feature = "ic-debug-print"))]
fn emit_debug_print(line: String) {
    ic_cdk::api::debug_print(line);
}

#[cfg(all(not(target_arch = "wasm32"), not(feature = "ic-debug-print")))]
fn emit_debug_print(_line: String) {}

#[cfg(not(target_arch = "wasm32"))]
fn emit_bounded_log_line(line: &str) {
    const MAX_LOG_LINE_BYTES: usize = 16 * 1024;
    if line.len() <= MAX_LOG_LINE_BYTES {
        emit_debug_print(line.to_string());
        return;
    }
    let mut prefix = String::new();
    for ch in line.chars() {
        let next_len = prefix.len() + ch.len_utf8();
        if next_len > 1024 {
            break;
        }
        prefix.push(ch);
    }
    let escaped = prefix
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r");
    let truncated_count = LOG_TRUNCATED_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    emit_debug_print(format!(
        "{{\"truncated\":true,\"truncated_count\":{},\"max_bytes\":{MAX_LOG_LINE_BYTES},\"original_bytes\":{},\"prefix\":\"{}\"}}",
        truncated_count,
        line.len(),
        escaped
    ));
}

fn apply_post_upgrade_migrations() {
    let meta = get_meta();
    let current = current_schema_version();
    if meta.schema_version > current {
        warn!(
            schema_version = meta.schema_version,
            supported_schema = current,
            "upgrade schema is newer than supported"
        );
        let mut next = meta;
        next.needs_migration = true;
        evm_db::meta::set_meta(next);
        return;
    }

    let from = meta.schema_version;
    if from < current || meta.needs_migration {
        set_needs_migration(true);
        set_schema_migration_state(SchemaMigrationState {
            phase: SchemaMigrationPhase::Init,
            cursor: 0,
            from_version: from,
            to_version: current,
            last_error: 0,
            cursor_key_set: false,
            cursor_key: [0u8; 32],
        });
        evm_db::stable_state::with_state_mut(|state| {
            let mut migration = *state.state_root_migration.get();
            if migration.phase == MigrationPhase::Done {
                migration.phase = MigrationPhase::Init;
                migration.cursor = 0;
                migration.last_error = 0;
                migration.schema_version_target = current_schema_version();
                state.state_root_migration.set(migration);
            }
        });
    }
    drive_migrations_tick(1024, 1024);
}

fn schedule_cycle_observer() {
    ic_cdk_timers::set_timer_interval(std::time::Duration::from_secs(60), || async {
        drive_migrations_tick(256, 512);
        let mode = observe_cycles();
        if mode != OpsMode::Critical && !migration_pending() {
            schedule_mining();
        }
    });
}

fn schedule_mining() {
    schedule_mining_with_interval(None);
}

fn schedule_mining_with_interval(override_interval_ms: Option<u64>) {
    if reject_write_reason().is_some() {
        return;
    }
    // RefCell再入防止: with_state_mut内は状態更新のみ。タイマー副作用は借用解放後に実行する。
    let interval_ms = evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        if !chain_state.auto_mine_enabled {
            return None;
        }
        if chain_state.mining_scheduled {
            return None;
        }
        chain_state.mining_scheduled = true;
        let interval_ms = override_interval_ms.unwrap_or(chain_state.mining_interval_ms);
        state.chain_state.set(chain_state);
        Some(interval_ms)
    });
    if let Some(interval_ms) = interval_ms {
        ic_cdk_timers::set_timer(std::time::Duration::from_millis(interval_ms), async move {
            mining_tick();
        });
    }
}

fn bump_mining_fail_streak() -> u32 {
    MINING_FAIL_STREAK.with(|cell| {
        let next = cell.get().saturating_add(1);
        cell.set(next);
        next
    })
}

fn reset_mining_fail_streak() {
    MINING_FAIL_STREAK.with(|cell| cell.set(0));
}

fn mining_backoff_interval_ms(base_interval_ms: u64, failures: u32) -> u64 {
    if failures == 0 {
        return base_interval_ms;
    }
    let shift = failures.min(16);
    let multiplier = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    let interval = base_interval_ms.saturating_mul(multiplier);
    interval.min(MAX_MINING_BACKOFF_MS).max(base_interval_ms)
}

fn schedule_prune() {
    // RefCell再入防止: with_state_mut内はフラグ更新のみ。タイマー副作用は外で実行する。
    let interval_ms = evm_db::stable_state::with_state_mut(|state| {
        let mut config = *state.prune_config.get();
        if !config.pruning_enabled {
            return None;
        }
        if config.prune_scheduled {
            return None;
        }
        config.prune_scheduled = true;
        let interval_ms = config.timer_interval_ms;
        state.prune_config.set(config);
        Some(interval_ms)
    });
    if let Some(interval_ms) = interval_ms {
        ic_cdk_timers::set_timer(std::time::Duration::from_millis(interval_ms), async move {
            pruning_tick();
        });
    }
}

fn pruning_tick() {
    let should_run = evm_db::stable_state::with_state_mut(|state| {
        let mut config = *state.prune_config.get();
        config.prune_scheduled = false;
        let enabled = config.pruning_enabled;
        state.prune_config.set(config);
        enabled
    });
    if should_run {
        if let Err(err) = chain::prune_tick() {
            PRUNE_ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
            error!(error = ?err, "prune_tick failed");
        }
    }
    schedule_prune();
}

fn mining_tick() {
    if reject_write_reason().is_some() {
        evm_db::stable_state::with_state_mut(|state| {
            let mut chain_state = *state.chain_state.get();
            chain_state.mining_scheduled = false;
            chain_state.is_producing = false;
            state.chain_state.set(chain_state);
        });
        return;
    }
    let should_produce = evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.mining_scheduled = false;
        if !chain_state.auto_mine_enabled {
            state.chain_state.set(chain_state);
            return false;
        }
        if chain_state.is_producing {
            state.chain_state.set(chain_state);
            return false;
        }
        if state.ready_queue.len() == 0 {
            state.chain_state.set(chain_state);
            return false;
        }
        chain_state.is_producing = true;
        state.chain_state.set(chain_state);
        true
    });

    if should_produce {
        let base_interval_ms =
            evm_db::stable_state::with_state(|state| state.chain_state.get().mining_interval_ms);
        let result = chain::produce_block(evm_db::chain_data::MAX_TXS_PER_BLOCK);

        evm_db::stable_state::with_state_mut(|state| {
            let mut chain_state = *state.chain_state.get();
            chain_state.is_producing = false;
            state.chain_state.set(chain_state);
        });
        let next_interval_ms = match result {
            Ok(_) => {
                reset_mining_fail_streak();
                base_interval_ms
            }
            Err(chain::ChainError::NoExecutableTx) | Err(chain::ChainError::QueueEmpty) => {
                let failures = bump_mining_fail_streak();
                mining_backoff_interval_ms(base_interval_ms, failures)
            }
            Err(err) => {
                MINING_ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                error!(error = ?err, "mining_tick produce_block failed");
                let failures = bump_mining_fail_streak();
                mining_backoff_interval_ms(base_interval_ms, failures)
            }
        };
        schedule_mining_with_interval(Some(next_interval_ms));
        return;
    }
    schedule_mining();
}

#[ic_cdk::query]
fn get_queue_snapshot(limit: u32, cursor: Option<u64>) -> QueueSnapshotView {
    let limit = usize::try_from(limit)
        .unwrap_or(0)
        .min(MAX_QUEUE_SNAPSHOT_LIMIT);
    let snapshot = chain::get_queue_snapshot(limit, cursor);
    let mut items = Vec::with_capacity(snapshot.items.len());
    for item in snapshot.items.into_iter() {
        items.push(QueueItemView {
            seq: item.seq,
            tx_id: item.tx_id.0.to_vec(),
            kind: tx_kind_to_view(item.kind),
        });
    }
    QueueSnapshotView {
        items,
        next_cursor: snapshot.next_cursor,
    }
}
ic_cdk::export_candid!();

// NOTE: build-time only; keep out of production surface area.
#[cfg(feature = "did-gen")]
pub fn export_did() -> String {
    __export_service()
}

#[cfg(test)]
mod tests {
    use super::{
        chain_submit_error_to_code, clamp_return_data, exec_error_to_code,
        inspect_payload_limit_for_method, inspect_policy_for_method, map_execute_chain_result,
        map_submit_chain_error, migration_pending, prune_boundary_for_number,
        receipt_lookup_status, reject_anonymous_principal, reject_write_reason, tx_id_from_bytes,
        EthLogFilterView, ExecuteTxError, GetLogsErrorView, INSPECT_METHOD_POLICIES,
        MINING_ERROR_COUNT, PRUNE_ERROR_COUNT,
    };
    use candid::Principal;
    use evm_core::chain::{ChainError, ExecResult};
    use evm_core::revm_exec::{ExecError, OpHaltReason, OpTransactionError};
    use evm_db::chain_data::constants::MAX_RETURN_DATA;
    use evm_db::chain_data::{MigrationPhase, TxId, TxLoc};
    use evm_db::meta::{
        current_schema_version, schema_migration_state, set_meta, set_needs_migration,
        set_schema_migration_state, SchemaMigrationPhase, SchemaMigrationState,
    };
    use evm_db::stable_state::init_stable_state;
    use std::collections::BTreeSet;

    #[test]
    fn clamp_return_data_rejects_oversize() {
        let data = vec![0u8; MAX_RETURN_DATA + 1];
        assert_eq!(clamp_return_data(data), None);
    }

    #[test]
    fn clamp_return_data_allows_limit() {
        let data = vec![7u8; MAX_RETURN_DATA];
        let out = clamp_return_data(data.clone());
        assert_eq!(out, Some(data));
    }

    #[test]
    fn tx_id_from_bytes_rejects_wrong_len() {
        let out = tx_id_from_bytes(vec![1u8; 31]);
        assert!(out.is_none());
    }

    #[test]
    fn tx_id_from_bytes_accepts_32() {
        let input = vec![9u8; 32];
        let out = tx_id_from_bytes(input.clone()).expect("tx_id");
        assert_eq!(out.0.to_vec(), input);
    }

    #[test]
    fn exec_error_codes_match_fixed_pattern() {
        let inputs = [
            Some(ExecError::Decode(
                evm_core::tx_decode::DecodeError::InvalidRlp,
            )),
            Some(ExecError::TxError(OpTransactionError::TxBuildFailed)),
            Some(ExecError::TxError(OpTransactionError::TxRejectedByPolicy)),
            Some(ExecError::TxError(OpTransactionError::TxPrecheckFailed)),
            Some(ExecError::TxError(OpTransactionError::TxExecutionFailed)),
            Some(ExecError::Revert),
            Some(ExecError::EvmHalt(OpHaltReason::OutOfGas)),
            Some(ExecError::EvmHalt(OpHaltReason::InvalidOpcode)),
            Some(ExecError::EvmHalt(OpHaltReason::StackOverflow)),
            Some(ExecError::EvmHalt(OpHaltReason::StackUnderflow)),
            Some(ExecError::EvmHalt(OpHaltReason::InvalidJump)),
            Some(ExecError::EvmHalt(
                OpHaltReason::StateChangeDuringStaticCall,
            )),
            Some(ExecError::EvmHalt(OpHaltReason::PrecompileError)),
            Some(ExecError::EvmHalt(OpHaltReason::Unknown)),
            Some(ExecError::InvalidGasFee),
            Some(ExecError::ResultTooLarge),
            Some(ExecError::ExecutionFailed),
            None,
        ];

        for err in inputs.iter() {
            let code = exec_error_to_code(err.as_ref());
            assert!(code.starts_with("exec."));
            assert!(is_machine_code(code), "unexpected code: {code}");
            assert!(!code.contains('{'));
            assert!(!code.contains('}'));
            assert!(!code.contains(':'));
        }
    }

    #[test]
    fn pr8_submit_error_code_mapping_matches_expected_set() {
        let table = [
            (ChainError::TxTooLarge, ("arg.tx_too_large", true)),
            (ChainError::DecodeFailed, ("arg.decode_failed", true)),
            (
                ChainError::UnsupportedTxKind,
                ("arg.unsupported_tx_kind", true),
            ),
            (ChainError::TxAlreadySeen, ("submit.tx_already_seen", false)),
            (ChainError::InvalidFee, ("submit.invalid_fee", false)),
            (ChainError::NonceTooLow, ("submit.nonce_too_low", false)),
            (ChainError::NonceGap, ("submit.nonce_gap", false)),
            (ChainError::NonceConflict, ("submit.nonce_conflict", false)),
            (ChainError::QueueFull, ("submit.queue_full", false)),
            (
                ChainError::SenderQueueFull,
                ("submit.sender_queue_full", false),
            ),
            (
                ChainError::PrincipalQueueFull,
                ("submit.principal_queue_full", false),
            ),
        ];
        for (input, (expected_code, expected_invalid_arg)) in table {
            let (kind, code) = chain_submit_error_to_code(&input).expect("code mapping");
            assert_eq!(code, expected_code);
            assert!(is_machine_code(code));
            match kind {
                super::TxApiErrorKind::InvalidArgument => assert!(expected_invalid_arg),
                super::TxApiErrorKind::Rejected => assert!(!expected_invalid_arg),
            }
        }
    }

    #[test]
    fn exec_error_to_code_matches_expected_set() {
        let code = exec_error_to_code(Some(&ExecError::Revert));
        assert_eq!(code, "exec.revert");
        let code = exec_error_to_code(Some(&ExecError::EvmHalt(OpHaltReason::Unknown)));
        assert_eq!(code, "exec.halt.unknown");
        let code = exec_error_to_code(Some(&ExecError::TxError(OpTransactionError::TxBuildFailed)));
        assert_eq!(code, "exec.tx.build_failed");
        let code = exec_error_to_code(Some(&ExecError::ResultTooLarge));
        assert_eq!(code, "exec.result.too_large");
    }

    #[test]
    fn status_zero_exec_result_is_not_rejected() {
        let result = map_execute_chain_result(Ok(ExecResult {
            tx_id: TxId([0u8; 32]),
            block_number: 1,
            tx_index: 0,
            status: 0,
            gas_used: 21_000,
            return_data: Vec::new(),
            final_status: "Revert".to_string(),
        }))
        .expect("status=0 should still be Ok");
        assert_eq!(result.status, 0);
    }

    #[test]
    fn exec_failed_maps_to_rejected_exec_code() {
        let err = map_execute_chain_result(Err(ChainError::ExecFailed(Some(ExecError::Revert))))
            .expect_err("exec failed should be rejected");
        match err {
            ExecuteTxError::Rejected(code) => assert_eq!(code, "exec.revert"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn pr8_execute_decode_failed_maps_to_arg_code() {
        let err = map_execute_chain_result(Err(ChainError::DecodeFailed))
            .expect_err("must reject decode");
        match err {
            ExecuteTxError::InvalidArgument(code) => assert_eq!(code, "arg.decode_failed"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn pr8_execute_precompile_error_maps_to_exec_code() {
        let err = map_execute_chain_result(Err(ChainError::ExecFailed(Some(ExecError::EvmHalt(
            OpHaltReason::PrecompileError,
        )))))
        .expect_err("must map to precompile code");
        match err {
            ExecuteTxError::Rejected(code) => assert_eq!(code, "exec.halt.precompile_error"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn pr8_unexpected_chain_error_maps_to_internal_unexpected() {
        let err = map_submit_chain_error(ChainError::QueueEmpty, "test_submit");
        match err {
            super::SubmitTxError::Internal(code) => assert_eq!(code, "internal.unexpected"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn inspect_allowlist_accepts_known_methods() {
        assert!(inspect_payload_limit_for_method("submit_ic_tx").is_some());
        assert!(inspect_payload_limit_for_method("set_pruning_enabled").is_some());
        assert!(inspect_payload_limit_for_method("set_miner_allowlist").is_some());
        assert!(inspect_payload_limit_for_method("set_block_gas_limit").is_some());
        assert!(inspect_payload_limit_for_method("set_instruction_soft_limit").is_some());
    }

    #[test]
    fn inspect_allowlist_rejects_unknown_methods() {
        assert!(inspect_payload_limit_for_method("unknown_method").is_none());
    }

    #[test]
    fn inspect_allowlist_matches_did_updates() {
        let did_methods = did_update_methods();
        for method in did_methods.iter() {
            assert!(
                inspect_payload_limit_for_method(method).is_some(),
                "did update method is missing in inspect allowlist: {method}"
            );
        }
        for method in inspect_allowlist_methods().iter() {
            assert!(
                did_methods.contains(*method),
                "inspect allowlist method is missing in did: {method}"
            );
        }
    }

    #[test]
    fn reject_anonymous_principal_blocks_anonymous() {
        let out = reject_anonymous_principal(Principal::anonymous());
        assert_eq!(out, Some("auth.anonymous_forbidden".to_string()));
    }

    #[test]
    fn reject_anonymous_principal_allows_non_anonymous() {
        let principal = Principal::self_authenticating(b"wrapper-test-caller");
        let out = reject_anonymous_principal(principal);
        assert_eq!(out, None);
    }

    #[test]
    fn reject_write_reason_stops_on_needs_migration() {
        init_stable_state();
        set_schema_migration_state(SchemaMigrationState::done());
        set_needs_migration(true);
        let reason = reject_write_reason().expect("needs_migration should block writes");
        assert_eq!(reason, "ops.write.needs_migration");
    }

    #[test]
    fn migration_pending_does_not_advance_schema_migration_state() {
        init_stable_state();
        set_needs_migration(false);
        set_schema_migration_state(SchemaMigrationState {
            phase: SchemaMigrationPhase::Init,
            cursor: 0,
            from_version: current_schema_version(),
            to_version: current_schema_version(),
            last_error: 0,
            cursor_key_set: false,
            cursor_key: [0u8; 32],
        });
        evm_db::stable_state::with_state_mut(|state| {
            let mut meta = *state.state_root_meta.get();
            meta.initialized = true;
            state.state_root_meta.set(meta);
            let mut migration = *state.state_root_migration.get();
            migration.phase = MigrationPhase::Done;
            migration.cursor = 0;
            migration.last_error = 0;
            state.state_root_migration.set(migration);
        });

        let before = schema_migration_state();
        assert_eq!(before.phase, SchemaMigrationPhase::Init);
        let pending = migration_pending();
        assert!(!pending);
        let after = schema_migration_state();
        assert_eq!(after.phase, SchemaMigrationPhase::Init);
        assert_eq!(after.cursor, before.cursor);
    }

    #[test]
    fn inspect_payload_limit_applies_per_method() {
        let tx_limit = inspect_payload_limit_for_method("submit_ic_tx").expect("tx limit");
        let manage_limit = inspect_payload_limit_for_method("set_miner_allowlist")
            .expect("manage limit should be configured");
        assert!(manage_limit > tx_limit);
        assert_eq!(
            inspect_payload_limit_for_method("rpc_eth_send_raw_transaction"),
            Some(tx_limit)
        );
        assert_eq!(
            inspect_payload_limit_for_method("produce_block"),
            Some(manage_limit)
        );
        assert_eq!(inspect_payload_limit_for_method("unknown_method"), None);
    }

    #[test]
    fn inspect_policy_table_has_unique_methods() {
        let mut methods = BTreeSet::new();
        for policy in INSPECT_METHOD_POLICIES {
            let inserted = methods.insert(policy.method);
            assert!(
                inserted,
                "duplicate inspect policy method: {}",
                policy.method
            );
        }
    }

    #[test]
    fn inspect_policy_allowed_and_limit_are_consistent() {
        for method in inspect_allowlist_methods() {
            assert!(
                inspect_payload_limit_for_method(method).is_some(),
                "payload limit missing for method: {method}"
            );
            assert!(inspect_policy_for_method(method).is_some());
        }
        assert!(inspect_payload_limit_for_method("unknown_method").is_none());
    }

    #[test]
    fn prune_boundary_for_number_returns_boundary_only_for_pruned_range() {
        init_stable_state();
        evm_db::stable_state::with_state_mut(|state| {
            let mut prune_state = *state.prune_state.get();
            prune_state.set_pruned_before(10);
            state.prune_state.set(prune_state);
        });
        assert_eq!(prune_boundary_for_number(10), Some(10));
        assert_eq!(prune_boundary_for_number(11), None);
    }

    #[test]
    fn receipt_lookup_status_returns_possibly_pruned_when_loc_is_gone() {
        init_stable_state();
        let tx_id = TxId([0x44; 32]);
        evm_db::stable_state::with_state_mut(|state| {
            let mut prune_state = *state.prune_state.get();
            prune_state.set_pruned_before(7);
            state.prune_state.set(prune_state);
        });
        let out = receipt_lookup_status(tx_id);
        match out {
            super::RpcReceiptLookupView::PossiblyPruned {
                pruned_before_block,
            } => {
                assert_eq!(pruned_before_block, 7);
            }
            other => panic!("unexpected status: {other:?}"),
        }
    }

    #[test]
    fn receipt_lookup_status_returns_pruned_when_loc_indicates_included_before_boundary() {
        init_stable_state();
        let tx_id = TxId([0x55; 32]);
        evm_db::stable_state::with_state_mut(|state| {
            state.tx_locs.insert(tx_id, TxLoc::included(5, 0));
            let mut prune_state = *state.prune_state.get();
            prune_state.set_pruned_before(8);
            state.prune_state.set(prune_state);
        });
        let out = receipt_lookup_status(tx_id);
        match out {
            super::RpcReceiptLookupView::Pruned {
                pruned_before_block,
            } => {
                assert_eq!(pruned_before_block, 8);
            }
            other => panic!("unexpected status: {other:?}"),
        }
    }

    #[test]
    fn get_ops_status_reports_error_counters() {
        init_stable_state();
        let before_mining = MINING_ERROR_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        let before_prune = PRUNE_ERROR_COUNT.load(std::sync::atomic::Ordering::Relaxed);
        MINING_ERROR_COUNT.fetch_add(2, std::sync::atomic::Ordering::Relaxed);
        PRUNE_ERROR_COUNT.fetch_add(3, std::sync::atomic::Ordering::Relaxed);
        let view = super::get_ops_status();
        assert!(view.mining_error_count >= before_mining.saturating_add(2));
        assert!(view.prune_error_count >= before_prune.saturating_add(3));
    }

    #[test]
    fn health_and_ops_status_expose_block_gas_limit() {
        init_stable_state();
        evm_db::stable_state::with_state_mut(|state| {
            let mut chain_state = *state.chain_state.get();
            chain_state.block_gas_limit = 9_000_000;
            chain_state.instruction_soft_limit = 123_456;
            state.chain_state.set(chain_state);
        });
        let health = super::health();
        assert_eq!(health.block_gas_limit, 9_000_000);
        assert_eq!(health.instruction_soft_limit, 123_456);
        let ops = super::get_ops_status();
        assert_eq!(ops.block_gas_limit, 9_000_000);
        assert_eq!(ops.instruction_soft_limit, 123_456);
    }

    #[test]
    fn meta_corruption_reflects_in_write_blocking_status() {
        init_stable_state();
        let mut meta = evm_db::meta::Meta::new();
        meta.needs_migration = true;
        set_meta(meta);
        let view = super::get_ops_status();
        assert!(view.needs_migration);
        assert_eq!(view.decode_failure_count, 0);
        assert_eq!(view.decode_failure_last_label, None);
        let reason = reject_write_reason().expect("write should be blocked");
        assert_eq!(reason, "ops.write.needs_migration");
    }

    #[test]
    fn decode_failure_label_view_prefers_ascii_machine_code() {
        let mut raw = [0u8; 32];
        raw[..12].copy_from_slice(b"block_data_1");
        let out = super::decode_failure_label_view(raw);
        assert_eq!(out, Some("block_data_1".to_string()));
    }

    #[test]
    fn decode_failure_label_view_falls_back_to_hex() {
        let mut raw = [0u8; 32];
        raw[0] = 0xff;
        raw[1] = 0x01;
        let out = super::decode_failure_label_view(raw).expect("hex label");
        assert!(out.starts_with("hex:"));
    }

    #[test]
    fn rpc_eth_get_logs_rejects_reverse_range() {
        init_stable_state();
        let err = super::rpc_eth_get_logs(EthLogFilterView {
            from_block: Some(10),
            to_block: Some(9),
            address: None,
            topic0: None,
            topic1: None,
            limit: Some(10),
        })
        .expect_err("reverse range should fail");
        assert_eq!(
            err,
            GetLogsErrorView::InvalidArgument("from_block must be <= to_block".to_string())
        );
    }

    #[test]
    fn rpc_eth_get_logs_rejects_range_too_large() {
        init_stable_state();
        let err = super::rpc_eth_get_logs(EthLogFilterView {
            from_block: Some(0),
            to_block: Some(6_001),
            address: None,
            topic0: None,
            topic1: None,
            limit: Some(10),
        })
        .expect_err("wide range should fail");
        assert_eq!(err, GetLogsErrorView::RangeTooLarge);
    }

    #[test]
    fn rpc_eth_get_logs_rejects_unsupported_topic1_filter() {
        init_stable_state();
        let err = super::rpc_eth_get_logs(EthLogFilterView {
            from_block: Some(0),
            to_block: Some(0),
            address: None,
            topic0: None,
            topic1: Some(vec![0u8; 32]),
            limit: Some(10),
        })
        .expect_err("topic1 should be unsupported");
        assert_eq!(
            err,
            GetLogsErrorView::UnsupportedFilter("topic1 is not supported".to_string())
        );
    }

    #[test]
    fn rpc_eth_get_logs_rejects_over_limit() {
        init_stable_state();
        let err = super::rpc_eth_get_logs(EthLogFilterView {
            from_block: Some(0),
            to_block: Some(0),
            address: None,
            topic0: None,
            topic1: None,
            limit: Some(2_001),
        })
        .expect_err("limit should fail");
        assert_eq!(err, GetLogsErrorView::TooManyResults);
    }

    #[cfg(feature = "dev-faucet")]
    #[test]
    fn inspect_allowlist_accepts_dev_mint_with_feature() {
        assert!(inspect_payload_limit_for_method("dev_mint").is_some());
    }

    fn is_machine_code(value: &str) -> bool {
        value
            .chars()
            .all(|ch| ch == '.' || ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
    }

    fn inspect_allowlist_methods() -> BTreeSet<&'static str> {
        let mut out = BTreeSet::new();
        for policy in INSPECT_METHOD_POLICIES {
            out.insert(policy.method);
        }
        #[cfg(feature = "dev-faucet")]
        out.insert("dev_mint");
        out
    }

    fn did_update_methods() -> BTreeSet<String> {
        let did = include_str!("../evm_canister.did");
        let mut out = BTreeSet::new();
        let mut in_service = false;
        let mut stmt = String::new();
        for line in did.lines() {
            let trimmed = line.trim();
            if !in_service {
                if trimmed.starts_with("service ") || trimmed.starts_with("service:") {
                    in_service = true;
                }
                continue;
            }
            if trimmed == "}" {
                break;
            }
            if trimmed.is_empty() {
                continue;
            }
            if !stmt.is_empty() {
                stmt.push(' ');
            }
            stmt.push_str(trimmed);
            if !trimmed.ends_with(';') {
                continue;
            }
            if stmt.contains(" : (") && stmt.contains("-> (") && !stmt.contains(" query") {
                if let Some((name, _)) = stmt.split_once(" : (") {
                    out.insert(name.trim().to_string());
                }
            }
            stmt.clear();
        }
        out
    }

    #[test]
    fn with_state_mut_blocks_avoid_async_and_timer_side_effects() {
        let source = include_str!("lib.rs");
        for (start, _) in source.match_indices("with_state_mut(|") {
            let tail = &source[start..];
            let Some(rel_end) = tail.find("});") else {
                continue;
            };
            let end = start + rel_end + 3;
            let segment = &source[start..end];
            assert!(
                !segment.contains("ic_cdk_timers::set_timer("),
                "set_timer must be outside with_state_mut block"
            );
            assert!(
                !segment.contains("ic_cdk_timers::set_timer_interval("),
                "set_timer_interval must be outside with_state_mut block"
            );
            assert!(
                !segment.contains(".await"),
                "await must not appear inside with_state_mut block"
            );
        }
    }
}
