//! どこで: canister入口 / 何を: Phase1のAPI公開 / なぜ: submit中心の安全な運用導線を提供するため

use candid::{CandidType, Principal};
use evm_core::{chain, hash};
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::chain_data::constants::{MAX_QUEUE_SNAPSHOT_LIMIT, MAX_RETURN_DATA, MAX_TX_SIZE};
use evm_db::chain_data::{
    BlockData, CallerKey, MigrationPhase, OpsConfigV1, OpsMode, ReceiptLike, StoredTx,
    StoredTxBytes, TxId, TxKind, TxLoc, TxLocKind, LOG_CONFIG_FILTER_MAX,
};
use evm_db::meta::{
    current_schema_version, ensure_meta_initialized, get_meta, mark_migration_applied,
    schema_migration_state, set_needs_migration, set_schema_migration_state, set_tx_locs_v3_active,
    SchemaMigrationPhase, SchemaMigrationState,
};
use evm_db::stable_state::{init_stable_state, with_state};
use evm_db::upgrade;
use ic_cdk::api::{
    accept_message, canister_cycle_balance, env_var_name_exists, env_var_value, is_controller,
    msg_caller, msg_method_name, time,
};
use serde::Deserialize;
use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tracing::{error, warn};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

#[cfg(target_arch = "wasm32")]
getrandom::register_custom_getrandom!(always_fail_getrandom);

#[cfg(target_arch = "wasm32")]
fn always_fail_getrandom(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExecResultDto {
    pub tx_id: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub return_data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum LookupError {
    NotFound,
    Pending,
    Pruned { pruned_before_block: u64 },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ProduceBlockStatus {
    Produced {
        block_number: u64,
        txs: u32,
        gas_used: u64,
        dropped: u32,
    },
    NoOp {
        reason: NoOpReason,
    },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum NoOpReason {
    NoExecutableTx,
    CycleCritical,
    NeedsMigration,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ProduceBlockError {
    Internal(String),
    InvalidArgument(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct OpsConfigView {
    pub low_watermark: u128,
    pub critical: u128,
    pub freeze_on_critical: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum OpsModeView {
    Normal,
    Low,
    Critical,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct OpsStatusView {
    pub config: OpsConfigView,
    pub last_cycle_balance: u128,
    pub last_check_ts: u64,
    pub mode: OpsModeView,
    pub safe_stop_latched: bool,
    pub needs_migration: bool,
    pub schema_version: u32,
    pub log_filter_override: Option<String>,
    pub log_truncated_count: u64,
    pub critical_corrupt: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum SubmitTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ExecuteTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct BlockView {
    pub number: u64,
    pub parent_hash: Vec<u8>,
    pub block_hash: Vec<u8>,
    pub timestamp: u64,
    pub tx_ids: Vec<Vec<u8>>,
    pub tx_list_hash: Vec<u8>,
    pub state_root: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ReceiptView {
    pub tx_id: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub effective_gas_price: u64,
    pub l1_data_fee: u128,
    pub operator_fee: u128,
    pub total_fee: u128,
    pub return_data_hash: Vec<u8>,
    pub return_data: Option<Vec<u8>>,
    pub contract_address: Option<Vec<u8>>,
    pub logs: Vec<LogView>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct LogView {
    pub address: Vec<u8>,
    pub topics: Vec<Vec<u8>>,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QueueItemView {
    pub seq: u64,
    pub tx_id: Vec<u8>,
    pub kind: TxKindView,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QueueSnapshotView {
    pub items: Vec<QueueItemView>,
    pub next_cursor: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct HealthView {
    pub tip_number: u64,
    pub tip_hash: Vec<u8>,
    pub last_block_time: u64,
    pub queue_len: u64,
    pub auto_mine_enabled: bool,
    pub is_producing: bool,
    pub mining_scheduled: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DropCountView {
    pub code: u16,
    pub count: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct MetricsView {
    pub window: u64,
    pub blocks: u64,
    pub txs: u64,
    pub avg_txs_per_block: u64,
    pub block_rate_per_sec_x1000: Option<u64>,
    pub ema_block_rate_per_sec_x1000: u64,
    pub ema_txs_per_block_x1000: u64,
    pub queue_len: u64,
    pub drop_counts: Vec<DropCountView>,
    pub total_submitted: u64,
    pub total_included: u64,
    pub total_dropped: u64,
    pub cycles: u128,
    pub pruned_before_block: Option<u64>,
    pub dev_faucet_enabled: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct PruneResultView {
    pub did_work: bool,
    pub remaining: u64,
    pub pruned_before_block: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum EthTxListView {
    Hashes(Vec<Vec<u8>>),
    Full(Vec<EthTxView>),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthBlockView {
    pub number: u64,
    pub parent_hash: Vec<u8>,
    pub block_hash: Vec<u8>,
    pub timestamp: u64,
    pub txs: EthTxListView,
    pub state_root: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthTxView {
    pub hash: Vec<u8>,
    pub eth_tx_hash: Option<Vec<u8>>,
    pub kind: TxKindView,
    pub raw: Vec<u8>,
    pub decoded: Option<DecodedTxView>,
    pub decode_ok: bool,
    pub block_number: Option<u64>,
    pub tx_index: Option<u32>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DecodedTxView {
    pub from: Vec<u8>,
    pub to: Option<Vec<u8>>,
    pub nonce: u64,
    pub value: Vec<u8>,
    pub input: Vec<u8>,
    pub gas_limit: u64,
    pub gas_price: u128,
    pub chain_id: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct EthReceiptView {
    pub tx_hash: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub status: u8,
    pub gas_used: u64,
    pub effective_gas_price: u64,
    pub l1_data_fee: u128,
    pub operator_fee: u128,
    pub total_fee: u128,
    pub contract_address: Option<Vec<u8>>,
    pub logs: Vec<LogView>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExportCursorView {
    pub block_number: u64,
    pub segment: u8,
    pub byte_offset: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExportChunkView {
    pub segment: u8,
    pub start: u32,
    pub bytes: Vec<u8>,
    pub payload_len: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct ExportResponseView {
    pub chunks: Vec<ExportChunkView>,
    pub next_cursor: Option<ExportCursorView>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum ExportErrorView {
    InvalidCursor { message: String },
    Pruned { pruned_before_block: u64 },
    MissingData { message: String },
    Limit,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum PendingStatusView {
    Queued { seq: u64 },
    Included { block_number: u64, tx_index: u32 },
    Dropped { code: u16 },
    Unknown,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub enum TxKindView {
    EthSigned,
    IcSynthetic,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct PrunePolicyView {
    pub target_bytes: u64,
    pub retain_days: u64,
    pub retain_blocks: u64,
    pub headroom_ratio_bps: u32,
    pub hard_emergency_ratio_bps: u32,
    pub timer_interval_ms: u64,
    pub max_ops_per_tick: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct PruneStatusView {
    pub pruning_enabled: bool,
    pub prune_running: bool,
    pub estimated_kept_bytes: u64,
    pub high_water_bytes: u64,
    pub low_water_bytes: u64,
    pub hard_emergency_bytes: u64,
    pub last_prune_at: u64,
    pub pruned_before_block: Option<u64>,
    pub oldest_kept_block: Option<u64>,
    pub oldest_kept_timestamp: Option<u64>,
    pub need_prune: bool,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct GenesisBalanceView {
    pub address: Vec<u8>,
    pub amount: u128,
}

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

#[ic_cdk::init]
fn init(args: Option<InitArgs>) {
    init_stable_state();
    let _ = ensure_meta_initialized();
    init_tracing();
    let args = args.unwrap_or_else(|| {
        ic_cdk::trap("InitArgsRequired: InitArgs is required; pass (opt record {...})")
    });
    if let Err(reason) = args.validate() {
        ic_cdk::trap(&format!("InvalidInitArgs: {reason}"));
    }
    for alloc in args.genesis_balances.iter() {
        let mut addr = [0u8; 20];
        addr.copy_from_slice(&alloc.address);
        chain::dev_mint(addr, alloc.amount)
            .unwrap_or_else(|_| ic_cdk::trap("init: genesis mint failed"));
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
    if !inspect_method_allowed(method.as_str()) {
        return;
    }
    if reject_anonymous_update().is_some() {
        return;
    }
    let payload_len = inspect_payload_len();
    if payload_len <= MAX_TX_SIZE.saturating_mul(2) {
        accept_message();
    }
}

fn inspect_method_allowed(method: &str) -> bool {
    if cfg!(feature = "dev-faucet") && method == "dev_mint" {
        return true;
    }
    matches!(
        method,
        "submit_eth_tx"
            | "submit_ic_tx"
            | "rpc_eth_send_raw_transaction"
            | "set_auto_mine"
            | "set_mining_interval_ms"
            | "set_prune_policy"
            | "set_pruning_enabled"
            | "set_ops_config"
            | "set_log_filter"
            | "set_miner_allowlist"
            | "prune_blocks"
            | "produce_block"
    )
}

#[allow(deprecated)]
fn inspect_payload_len() -> usize {
    ic_cdk::api::call::arg_data_raw_size()
}

#[cfg(test)]
fn map_execute_chain_result(
    result: Result<chain::ExecResult, chain::ChainError>,
) -> Result<ExecResultDto, ExecuteTxError> {
    let result = match result {
        Ok(value) => value,
        Err(chain::ChainError::DecodeFailed) => {
            return Err(ExecuteTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::TxTooLarge) => {
            return Err(ExecuteTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(ExecuteTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(ExecuteTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceTooLow) => {
            return Err(ExecuteTxError::Rejected("nonce too low".to_string()));
        }
        Err(chain::ChainError::NonceGap) => {
            return Err(ExecuteTxError::Rejected("nonce gap".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(ExecuteTxError::Rejected("nonce conflict".to_string()));
        }
        Err(chain::ChainError::QueueFull) => {
            return Err(ExecuteTxError::Rejected("queue full".to_string()));
        }
        Err(chain::ChainError::SenderQueueFull) => {
            return Err(ExecuteTxError::Rejected("sender queue full".to_string()));
        }
        Err(chain::ChainError::ExecFailed(err)) => {
            let code = exec_error_to_code(err.as_ref());
            return Err(ExecuteTxError::Rejected(code.to_string()));
        }
        Err(err) => {
            error!(error = ?err, "execute_tx failed");
            return Err(ExecuteTxError::Internal("internal error".to_string()));
        }
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
    let tx_id = match chain::submit_tx_in(chain::TxIn::EthSigned(raw_tx)) {
        Ok(value) => value,
        Err(chain::ChainError::TxTooLarge) => {
            return Err(SubmitTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::DecodeFailed) => {
            return Err(SubmitTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::UnsupportedTxKind) => {
            return Err(SubmitTxError::InvalidArgument(
                "unsupported tx kind".to_string(),
            ));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(SubmitTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(SubmitTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceTooLow) => {
            return Err(SubmitTxError::Rejected("nonce too low".to_string()));
        }
        Err(chain::ChainError::NonceGap) => {
            return Err(SubmitTxError::Rejected("nonce gap".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(SubmitTxError::Rejected("nonce conflict".to_string()));
        }
        Err(chain::ChainError::QueueFull) => {
            return Err(SubmitTxError::Rejected("queue full".to_string()));
        }
        Err(chain::ChainError::SenderQueueFull) => {
            return Err(SubmitTxError::Rejected("sender queue full".to_string()));
        }
        Err(err) => {
            error!(error = ?err, "submit_eth_tx failed");
            return Err(SubmitTxError::Internal("internal error".to_string()));
        }
    };
    schedule_mining();
    Ok(tx_id.0.to_vec())
}

#[ic_cdk::update]
fn submit_ic_tx(tx_bytes: Vec<u8>) -> Result<Vec<u8>, SubmitTxError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(SubmitTxError::Rejected(reason));
    }
    if let Some(reason) = reject_write_reason() {
        return Err(SubmitTxError::Rejected(reason));
    }
    let caller_principal = ic_cdk::api::msg_caller().as_slice().to_vec();
    let canister_id = ic_cdk::api::canister_self().as_slice().to_vec();
    let tx_id = match chain::submit_tx_in(chain::TxIn::IcSynthetic {
        caller_principal,
        canister_id,
        tx_bytes,
    }) {
        Ok(value) => value,
        Err(chain::ChainError::TxTooLarge) => {
            return Err(SubmitTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::DecodeFailed) => {
            return Err(SubmitTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::UnsupportedTxKind) => {
            return Err(SubmitTxError::InvalidArgument(
                "unsupported tx kind".to_string(),
            ));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(SubmitTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(SubmitTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceTooLow) => {
            return Err(SubmitTxError::Rejected("nonce too low".to_string()));
        }
        Err(chain::ChainError::NonceGap) => {
            return Err(SubmitTxError::Rejected("nonce gap".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(SubmitTxError::Rejected("nonce conflict".to_string()));
        }
        Err(chain::ChainError::QueueFull) => {
            return Err(SubmitTxError::Rejected("queue full".to_string()));
        }
        Err(chain::ChainError::SenderQueueFull) => {
            return Err(SubmitTxError::Rejected("sender queue full".to_string()));
        }
        Err(err) => {
            error!(error = ?err, "submit_ic_tx failed");
            return Err(SubmitTxError::Internal("internal error".to_string()));
        }
    };
    schedule_mining();
    Ok(tx_id.0.to_vec())
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
        Ok(block) => {
            let gas_used = with_state(|_state| {
                let mut total = 0u64;
                for tx_id in block.tx_ids.iter() {
                    if let Some(receipt) = chain::get_receipt(tx_id) {
                        total = total.saturating_add(receipt.gas_used);
                    }
                }
                total
            });
            let dropped = with_state(|state| {
                let metrics = *state.metrics_state.get();
                let mut count = 0u64;
                for bucket in metrics.buckets.iter() {
                    if bucket.block_number == block.number {
                        count = bucket.drops;
                        break;
                    }
                }
                count
            });
            Ok(ProduceBlockStatus::Produced {
                block_number: block.number,
                txs: block.tx_ids.len().try_into().unwrap_or(u32::MAX),
                gas_used,
                dropped: dropped.try_into().unwrap_or(u32::MAX),
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
    let block = chain::get_block(number)?;
    let txs = if full_tx {
        let mut list = Vec::with_capacity(block.tx_ids.len());
        for tx_id in block.tx_ids.iter() {
            if let Some(view) = tx_to_view(*tx_id) {
                list.push(view);
            }
        }
        EthTxListView::Full(list)
    } else {
        EthTxListView::Hashes(block.tx_ids.iter().map(|id| id.0.to_vec()).collect())
    };
    Some(EthBlockView {
        number: block.number,
        parent_hash: block.parent_hash.to_vec(),
        block_hash: block.block_hash.to_vec(),
        timestamp: block.timestamp,
        txs,
        state_root: block.state_root.to_vec(),
    })
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_by_hash(tx_hash: Vec<u8>) -> Option<EthTxView> {
    let tx_id = tx_id_from_bytes(tx_hash)?;
    tx_to_view(tx_id)
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_receipt(tx_hash: Vec<u8>) -> Option<EthReceiptView> {
    let tx_id = tx_id_from_bytes(tx_hash)?;
    let receipt = chain::get_receipt(&tx_id)?;
    Some(receipt_to_eth_view(receipt))
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
    let tx_id = match chain::submit_tx_in(chain::TxIn::EthSigned(raw_tx)) {
        Ok(value) => value,
        Err(chain::ChainError::TxTooLarge) => {
            return Err(SubmitTxError::InvalidArgument("tx too large".to_string()));
        }
        Err(chain::ChainError::DecodeFailed) => {
            return Err(SubmitTxError::InvalidArgument("decode failed".to_string()));
        }
        Err(chain::ChainError::UnsupportedTxKind) => {
            return Err(SubmitTxError::InvalidArgument(
                "unsupported tx kind".to_string(),
            ));
        }
        Err(chain::ChainError::TxAlreadySeen) => {
            return Err(SubmitTxError::Rejected("tx already seen".to_string()));
        }
        Err(chain::ChainError::InvalidFee) => {
            return Err(SubmitTxError::Rejected("invalid fee".to_string()));
        }
        Err(chain::ChainError::NonceTooLow) => {
            return Err(SubmitTxError::Rejected("nonce too low".to_string()));
        }
        Err(chain::ChainError::NonceGap) => {
            return Err(SubmitTxError::Rejected("nonce gap".to_string()));
        }
        Err(chain::ChainError::NonceConflict) => {
            return Err(SubmitTxError::Rejected("nonce conflict".to_string()));
        }
        Err(chain::ChainError::QueueFull) => {
            return Err(SubmitTxError::Rejected("queue full".to_string()));
        }
        Err(chain::ChainError::SenderQueueFull) => {
            return Err(SubmitTxError::Rejected("sender queue full".to_string()));
        }
        Err(err) => {
            error!(error = ?err, "rpc_eth_send_raw_transaction failed");
            return Err(SubmitTxError::Internal("internal error".to_string()));
        }
    };
    schedule_mining();
    Ok(tx_id.0.to_vec())
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
        OpsStatusView {
            config: OpsConfigView {
                low_watermark: config.low_watermark,
                critical: config.critical,
                freeze_on_critical: config.freeze_on_critical,
            },
            last_cycle_balance: ops.last_cycle_balance,
            last_check_ts: ops.last_check_ts,
            mode: mode_to_view(ops.mode),
            safe_stop_latched: ops.safe_stop_latched,
            needs_migration: meta.needs_migration,
            schema_version: meta.schema_version,
            log_filter_override: state.log_config.get().filter().map(str::to_string),
            log_truncated_count: LOG_TRUNCATED_COUNT.load(Ordering::Relaxed),
            critical_corrupt: critical_corrupt_state(),
        }
    })
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

fn tx_id_from_bytes(tx_id: Vec<u8>) -> Option<TxId> {
    if tx_id.len() != 32 {
        return None;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    Some(TxId(buf))
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
                input: decoded.input,
                gas_limit: decoded.gas_limit,
                gas_price: decoded.gas_price,
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
        kind: tx_kind_to_view(kind),
        raw: stored.raw.clone(),
        decode_ok: decoded.is_some(),
        decoded,
        block_number,
        tx_index,
    })
}

fn receipt_to_eth_view(receipt: ReceiptLike) -> EthReceiptView {
    EthReceiptView {
        tx_hash: receipt.tx_id.0.to_vec(),
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

fn mode_to_view(mode: OpsMode) -> OpsModeView {
    match mode {
        OpsMode::Normal => OpsModeView::Normal,
        OpsMode::Low => OpsModeView::Low,
        OpsMode::Critical => OpsModeView::Critical,
    }
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
    if !schema_migration_tick(32) {
        return true;
    }
    let meta = get_meta();
    if meta.needs_migration || meta.schema_version < current_schema_version() {
        return true;
    }
    let pending =
        with_state(|state| state.state_root_migration.get().phase != MigrationPhase::Done);
    if !pending {
        return false;
    }
    !chain::state_root_migration_tick(512)
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
        let mut ops = *state.ops_state.get();
        let next_mode = if balance < config.critical {
            if config.freeze_on_critical {
                ops.safe_stop_latched = true;
            }
            OpsMode::Critical
        } else if ops.safe_stop_latched
            && config.freeze_on_critical
            && balance < config.low_watermark
        {
            OpsMode::Critical
        } else {
            if balance >= config.low_watermark {
                ops.safe_stop_latched = false;
            }
            if balance < config.low_watermark {
                OpsMode::Low
            } else {
                OpsMode::Normal
            }
        };
        ops.last_cycle_balance = balance;
        ops.last_check_ts = now;
        ops.mode = next_mode;
        let _ = state.ops_state.set(ops);
        next_mode
    })
}

fn cycle_mode() -> OpsMode {
    observe_cycles()
}

fn reject_write_reason() -> Option<String> {
    if migration_pending() {
        return Some("ops.write.needs_migration".to_string());
    }
    if cycle_mode() == OpsMode::Critical {
        return Some("ops.write.cycle_critical".to_string());
    }
    None
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

fn init_tracing() {
    static LOG_INIT: OnceLock<()> = OnceLock::new();
    let _ = LOG_INIT.get_or_init(|| {
        let env_filter = EnvFilter::new(resolve_log_filter().unwrap_or_else(|| "info".to_string()));
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

fn resolve_log_filter() -> Option<String> {
    if let Some(value) = read_env_var_guarded("LOG_FILTER", LOG_FILTER_MAX_LEN) {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    with_state(|state| state.log_config.get().filter().map(str::to_string))
}

const MAX_ENV_VAR_NAME_LEN: usize = 128;
const LOG_FILTER_MAX_LEN: usize = 256;
static LOG_TRUNCATED_COUNT: AtomicU64 = AtomicU64::new(0);

fn read_env_var_guarded(name: &str, max_value_len: usize) -> Option<String> {
    if name.len() > MAX_ENV_VAR_NAME_LEN {
        return None;
    }
    if !env_var_name_exists(name) {
        return None;
    }
    let value = env_var_value(name);
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

#[derive(Clone, Copy)]
struct IcDebugPrintMakeWriter;

impl<'a> MakeWriter<'a> for IcDebugPrintMakeWriter {
    type Writer = IcDebugPrintWriter;

    fn make_writer(&'a self) -> Self::Writer {
        IcDebugPrintWriter {
            buffer: String::new(),
        }
    }
}

struct IcDebugPrintWriter {
    buffer: String,
}

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

impl Drop for IcDebugPrintWriter {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

fn emit_complete_lines(buffer: &mut String) {
    static REENTRANT_GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    let guard = REENTRANT_GUARD.get_or_init(|| Mutex::new(())).lock();
    if guard.is_err() {
        if !buffer.is_empty() {
            ic_cdk::api::debug_print("{\"target\":\"tracing\",\"fallback\":true}".to_string());
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

fn emit_bounded_log_line(line: &str) {
    const MAX_LOG_LINE_BYTES: usize = 16 * 1024;
    if line.len() <= MAX_LOG_LINE_BYTES {
        ic_cdk::api::debug_print(line.to_string());
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
    ic_cdk::api::debug_print(format!(
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
    if migration_pending() {
        let _ = chain::state_root_migration_tick(1024);
        let _ = schema_migration_tick(1024);
    }
}

fn schedule_cycle_observer() {
    ic_cdk_timers::set_timer_interval(std::time::Duration::from_secs(60), || async {
        let mode = observe_cycles();
        if mode != OpsMode::Critical && !migration_pending() {
            schedule_mining();
        }
    });
}

fn schedule_mining() {
    if reject_write_reason().is_some() {
        return;
    }
    let interval_ms = evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        if !chain_state.auto_mine_enabled {
            return None;
        }
        if chain_state.mining_scheduled {
            return None;
        }
        chain_state.mining_scheduled = true;
        let interval_ms = chain_state.mining_interval_ms;
        state.chain_state.set(chain_state);
        Some(interval_ms)
    });
    if let Some(interval_ms) = interval_ms {
        ic_cdk_timers::set_timer(std::time::Duration::from_millis(interval_ms), async move {
            mining_tick();
        });
    }
}

fn schedule_prune() {
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
        let _ = chain::prune_tick();
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
        let _ = chain::produce_block(evm_db::chain_data::MAX_TXS_PER_BLOCK);

        evm_db::stable_state::with_state_mut(|state| {
            let mut chain_state = *state.chain_state.get();
            chain_state.is_producing = false;
            state.chain_state.set(chain_state);
        });
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
        clamp_return_data, exec_error_to_code, inspect_method_allowed, map_execute_chain_result,
        reject_anonymous_principal, tx_id_from_bytes, ExecuteTxError,
    };
    use candid::Principal;
    use evm_core::chain::{ChainError, ExecResult};
    use evm_core::revm_exec::{ExecError, OpHaltReason, OpTransactionError};
    use evm_db::chain_data::constants::MAX_RETURN_DATA;
    use evm_db::chain_data::TxId;

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
            Some(ExecError::ExecutionFailed),
            None,
        ];

        for err in inputs.iter() {
            let code = exec_error_to_code(err.as_ref());
            assert!(is_exec_code(code), "unexpected code: {code}");
            assert!(!code.contains('{'));
            assert!(!code.contains('}'));
            assert!(!code.contains(':'));
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
    fn inspect_allowlist_accepts_known_methods() {
        assert!(inspect_method_allowed("submit_ic_tx"));
        assert!(inspect_method_allowed("set_pruning_enabled"));
        assert!(inspect_method_allowed("set_miner_allowlist"));
    }

    #[test]
    fn inspect_allowlist_rejects_unknown_methods() {
        assert!(!inspect_method_allowed("unknown_method"));
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

    #[cfg(feature = "dev-faucet")]
    #[test]
    fn inspect_allowlist_accepts_dev_mint_with_feature() {
        assert!(inspect_method_allowed("dev_mint"));
    }

    fn is_exec_code(value: &str) -> bool {
        if !value.starts_with("exec.") {
            return false;
        }
        value
            .chars()
            .all(|ch| ch == '.' || ch == '_' || ch.is_ascii_lowercase() || ch.is_ascii_digit())
    }
}
