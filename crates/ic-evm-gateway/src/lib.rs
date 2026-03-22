//! どこで: canister入口 / 何を: Phase1のAPI公開 / なぜ: submit中心の安全な運用導線を提供するため

use candid::{CandidType, Nat, Principal};
use evm_core::chain;
use evm_core::hash;
use evm_core::wrap_precompile::unwrap_intent_from_log;
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::chain_data::constants::{MAX_QUEUE_SNAPSHOT_LIMIT, MAX_RETURN_DATA, MAX_TX_SIZE};
use evm_db::chain_data::runtime_defaults::{DEFAULT_BLOCK_GAS_LIMIT, DEFAULT_MIN_FEE_FLOOR};
use evm_db::chain_data::DEFAULT_MINING_INTERVAL_MS;
use evm_db::chain_data::MIN_PRUNE_MAX_OPS_PER_TICK;
use evm_db::chain_data::{
    BlockData, MigrationPhase, OpsMode, ReceiptLike, RuntimeConfigV1, TxId, TxKind, TxLoc,
    TxLocKind, UnwrapDispatchRequest, UnwrapRequestStatus, LOG_CONFIG_FILTER_MAX,
    UNWRAP_DECODE_FAILURE_CODE,
};
use evm_db::memory::{all_memory_regions, memory_size_pages, WASM_PAGE_SIZE_BYTES};
use evm_db::meta::{
    current_schema_version, ensure_meta_initialized, get_meta, mark_migration_applied,
    schema_migration_state, set_needs_migration, set_schema_migration_state, set_tx_locs_v3_active,
    SchemaMigrationPhase, SchemaMigrationState,
};
use evm_db::stable_state::{
    current_runtime_config, init_stable_state, set_runtime_config, with_state, with_state_mut,
};
use evm_db::upgrade;
use ic_cdk::api::{
    accept_message, canister_cycle_balance, is_controller, msg_caller, msg_method_name,
};
use num_bigint::BigUint;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::{error, info, warn};

#[cfg(not(target_arch = "wasm32"))]
use std::io::{self, Write};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Mutex, OnceLock};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
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

const PRUNE_EVENT_BLOCK_INTERVAL: u64 = 84;
const CYCLE_OBSERVER_FAST_INTERVAL_SECS: u64 = 60;
const CYCLE_OBSERVER_SLOW_INTERVAL_SECS: u64 = 3_600;
const WRAP_DISPATCH_DELAY_MS: u64 = 75;
const UNWRAP_QUARANTINE_ERROR: &str = "quarantine.decode.unwrap_request";

static UNWRAP_DISPATCH_SCHEDULED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

use ic_evm_rpc_types::*;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub genesis_balances: Vec<GenesisBalanceView>,
    pub wrap_canister_id: Principal,
    pub wrap_factory_address: Vec<u8>,
    pub query_instruction_soft_limit: Option<u64>,
    pub update_instruction_soft_limit: Option<u64>,
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
        if self.wrap_canister_id == Principal::anonymous() {
            return Err("wrap_canister_id must not be anonymous".to_string());
        }
        if self.wrap_factory_address.len() != 20 {
            return Err("wrap_factory_address must be 20 bytes".to_string());
        }
        if self.query_instruction_soft_limit == Some(0) {
            return Err("query_instruction_soft_limit must be > 0".to_string());
        }
        if self.update_instruction_soft_limit == Some(0) {
            return Err("update_instruction_soft_limit must be > 0".to_string());
        }
        Ok(())
    }

    fn runtime_config(&self) -> RuntimeConfigV1 {
        let mut wrap_factory_address = [0u8; 20];
        wrap_factory_address.copy_from_slice(&self.wrap_factory_address);
        RuntimeConfigV1::new(self.wrap_canister_id, wrap_factory_address)
    }
}

#[ic_cdk::init]
fn init(args: Option<InitArgs>) {
    init_inner(args, true);
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
            wrap_canister_id: Principal::anonymous(),
            wrap_factory_address: Vec::new(),
            query_instruction_soft_limit: None,
            update_instruction_soft_limit: None,
        })
    };
    if require_args || !args.genesis_balances.is_empty() {
        if let Err(reason) = args.validate() {
            ic_cdk::trap(format!("InvalidInitArgs: {reason}"));
        }
    }
    if require_args {
        set_runtime_config(args.runtime_config());
    }
    apply_instruction_soft_limits_from_init_args(&args);
    if !args.genesis_balances.is_empty() {
        for alloc in args.genesis_balances.iter() {
            let mut addr = [0u8; 20];
            addr.copy_from_slice(&alloc.address);
            chain::credit_balance(addr, alloc.amount)
                .unwrap_or_else(|_| ic_cdk::trap("init: genesis mint failed"));
        }
    }
    // 新規install直後でも write 系APIが migration_pending で止まらないよう、
    // 初期migrationを短いループで前進させる（状態が軽い初期導入を想定）。
    for _ in 0..8 {
        drive_migrations_tick(1024, 1024);
        if !migration_pending() {
            break;
        }
    }
    observe_cycles();
    schedule_mining();
    schedule_cycle_observer();
}

fn current_wrap_canister_id() -> Principal {
    current_runtime_config()
        .wrap_canister_id()
        .unwrap_or_else(|err| ic_cdk::trap(format!("InvalidRuntimeConfig: {err}")))
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<InitArgs>) {
    upgrade::post_upgrade();
    init_stable_state();
    let _ = ensure_meta_initialized();
    init_tracing();
    let args = args.unwrap_or_else(|| {
        ic_cdk::trap(
            "UpgradeArgsRequired: InitArgs is required on upgrade; pass (opt record {...})",
        )
    });
    if let Err(reason) = args.validate() {
        ic_cdk::trap(format!("InvalidInitArgs: {reason}"));
    }
    set_runtime_config(args.runtime_config());
    apply_instruction_soft_limits_from_init_args(&args);
    apply_post_upgrade_migrations();
    let (quarantined, dropped_from_dispatch_queue) =
        quarantine_decode_failed_unwrap_requests(current_time_nanos());
    if quarantined > 0 {
        warn!(
            quarantined,
            dropped_from_dispatch_queue, "post_upgrade quarantined decode-failed unwrap requests"
        );
    }
    observe_cycles();
    reset_mining_schedule_after_upgrade();
    restore_unwrap_dispatch_after_upgrade();
    schedule_mining();
    schedule_cycle_observer();
}

fn reset_mining_schedule_after_upgrade() {
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        // upgrade後はタイマー実体が失われるため、予約フラグを初期化して再登録可能にする。
        chain_state.mining_scheduled = false;
        state.chain_state.set(chain_state);
    });
}

fn restore_unwrap_dispatch_after_upgrade() {
    // upgrade後は timer 実体が失われるため、永続化済みの unwrap queue を再接続する。
    if recover_unwrap_dispatch_state_after_upgrade(current_time_nanos()) {
        schedule_unwrap_dispatch();
    }
}

fn current_time_nanos() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        // canister本番では IC の単調増加ナノ秒時刻を使う。
        ic_cdk::api::time()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // host test では SystemTime を使い、ic0::time 依存を避ける。
        // u64 へ収まらない値は clamp して production 側の型に合わせる。
        let nanos_u128 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let clamped = nanos_u128.min(u128::from(u64::MAX));
        u64::try_from(clamped).unwrap_or(u64::MAX)
    }
}

fn recover_unwrap_dispatch_state_after_upgrade(now: u64) -> bool {
    with_state_mut(|state| {
        let mut queued_ids = BTreeSet::new();
        for item in state.unwrap_dispatch_queue.range(..) {
            queued_ids.insert(item.value());
        }

        let mut candidates = Vec::new();
        for item in state.unwrap_requests.range(..) {
            let request_id = *item.key();
            let mut req = item.value().clone();
            match req.status {
                UnwrapRequestStatus::Queued => {
                    if !queued_ids.contains(&request_id) {
                        candidates.push((request_id, req));
                    }
                }
                UnwrapRequestStatus::Dispatching => {
                    req.status = UnwrapRequestStatus::Queued;
                    req.updated_at = now;
                    candidates.push((request_id, req));
                }
                UnwrapRequestStatus::Dispatched | UnwrapRequestStatus::DispatchFailed => {}
            }
        }

        if candidates.is_empty() {
            return !state.unwrap_dispatch_queue.is_empty();
        }

        let mut meta = *state.unwrap_dispatch_meta.get();
        for (request_id, req) in candidates {
            state.unwrap_requests.insert(request_id, req);
            if queued_ids.contains(&request_id) {
                continue;
            }
            let seq = meta.push();
            state.unwrap_dispatch_queue.insert(seq, request_id);
            queued_ids.insert(request_id);
        }
        state.unwrap_dispatch_meta.set(meta);
        !state.unwrap_dispatch_queue.is_empty()
    })
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
    if payload_len <= limit && inspect_lightweight_tx_guard(method.as_str()) {
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

const INSPECT_METHOD_POLICIES: &[InspectMethodPolicy] = &[
    InspectMethodPolicy {
        method: "submit_ic_tx",
        payload_limit: INSPECT_TX_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "rpc_eth_send_raw_transaction",
        payload_limit: INSPECT_TX_PAYLOAD_LIMIT,
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
        method: "set_log_filter",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "prune_blocks",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    #[cfg(feature = "precompile-profile-admin")]
    InspectMethodPolicy {
        method: "clear_precompile_profile",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    #[cfg(feature = "precompile-profile-admin")]
    InspectMethodPolicy {
        method: "profile_precompile_call",
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
    None
}

fn inspect_payload_len() -> usize {
    ic_cdk::api::msg_arg_data().len()
}

fn inspect_lightweight_tx_guard(method: &str) -> bool {
    // inspect_messageでは重い署名検証は行わず、軽量なフォーマット不正のみ早期除外する。
    if method != "rpc_eth_send_raw_transaction" {
        return true;
    }
    let raw = ic_cdk::api::msg_arg_data();
    let tx = match candid::decode_one::<Vec<u8>>(&raw) {
        Ok(value) => value,
        Err(_) => return false,
    };
    if tx.is_empty() {
        return false;
    }
    let first = tx[0];
    first != 0x03 && first != 0x04
}

#[ic_cdk::update]
fn submit_ic_tx(args: SubmitIcTxArgsDto) -> Result<Vec<u8>, SubmitTxError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(SubmitTxError::Rejected(reason));
    }
    if let Some(reason) = reject_write_reason() {
        return Err(SubmitTxError::Rejected(reason));
    }
    let tx = parse_submit_ic_tx_args(args)?;
    let out = ic_evm_rpc::submit_tx_in_with_code(
        chain::TxIn::IcSynthetic {
            caller_principal: ic_cdk::api::msg_caller().as_slice().to_vec(),
            canister_id: ic_cdk::api::canister_self().as_slice().to_vec(),
            tx,
        },
        "submit_ic_tx",
    );
    if out.is_ok() {
        schedule_mining();
    }
    out
}

#[ic_cdk::query]
fn estimate_ic_tx(args: SubmitIcTxArgsDto) -> Result<EstimateIcTxOk, ApiError> {
    let from = args
        .from
        .clone()
        .ok_or_else(|| api_invalid_argument("arg.from_required", "arg.from_required"))?;
    if from.len() != 20 {
        return Err(api_invalid_argument(
            "arg.from_invalid_length",
            "arg.from_invalid_length",
        ));
    }
    let tx = parse_submit_ic_tx_args(args).map_err(submit_tx_error_to_api_error)?;
    let suggested_max_priority_fee_per_gas =
        rpc_eth_max_priority_fee_per_gas().map_err(rpc_error_to_api_error)?;
    let suggested_max_fee_per_gas = rpc_eth_gas_price().map_err(rpc_error_to_api_error)?;
    let gas_limit = rpc_eth_estimate_gas_object(RpcCallObjectView {
        to: tx.to.map(|value| value.to_vec()),
        from: Some(from),
        gas: Some(tx.gas_limit),
        gas_price: None,
        nonce: Some(tx.nonce),
        max_fee_per_gas: Some(tx.max_fee_per_gas),
        max_priority_fee_per_gas: Some(tx.max_priority_fee_per_gas),
        chain_id: Some(CHAIN_ID),
        tx_type: None,
        access_list: None,
        value: Some(tx.value.to_vec()),
        data: Some(tx.data),
    })
    .map_err(rpc_error_to_api_error)?;
    Ok(EstimateIcTxOk {
        gas_limit,
        suggested_max_fee_per_gas,
        suggested_max_priority_fee_per_gas,
    })
}

fn parse_submit_ic_tx_args(
    args: SubmitIcTxArgsDto,
) -> Result<evm_core::tx_decode::IcSyntheticTxInput, SubmitTxError> {
    let to = match args.to {
        Some(bytes) => {
            if bytes.len() != 20 {
                return Err(SubmitTxError::InvalidArgument(
                    "arg.to_invalid_length".to_string(),
                ));
            }
            let mut out = [0u8; 20];
            out.copy_from_slice(&bytes);
            Some(out)
        }
        None => None,
    };
    let value = nat_to_fixed_be::<32>(&args.value)
        .ok_or_else(|| SubmitTxError::InvalidArgument("arg.value_out_of_range".to_string()))?;
    let max_fee_per_gas = nat_to_u128(&args.max_fee_per_gas)
        .ok_or_else(|| SubmitTxError::InvalidArgument("arg.fee_out_of_range".to_string()))?;
    let max_priority_fee_per_gas = nat_to_u128(&args.max_priority_fee_per_gas)
        .ok_or_else(|| SubmitTxError::InvalidArgument("arg.fee_out_of_range".to_string()))?;
    Ok(evm_core::tx_decode::IcSyntheticTxInput {
        to,
        value,
        gas_limit: args.gas_limit,
        nonce: args.nonce,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        data: args.data,
    })
}

fn api_invalid_argument(code: &str, message: &str) -> ApiError {
    ApiError::InvalidArgument(ApiErrorDetail {
        code: code.to_string(),
        message: message.to_string(),
    })
}

fn api_internal(code: &str, message: &str) -> ApiError {
    ApiError::Internal(ApiErrorDetail {
        code: code.to_string(),
        message: message.to_string(),
    })
}

fn submit_tx_error_to_api_error(err: SubmitTxError) -> ApiError {
    match err {
        SubmitTxError::InvalidArgument(code) => api_invalid_argument(&code, &code),
        SubmitTxError::Rejected(code) => ApiError::Rejected(ApiErrorDetail {
            code: code.clone(),
            message: code,
        }),
        SubmitTxError::Internal(code) => api_internal(&code, &code),
    }
}

fn rpc_error_to_api_error(err: RpcErrorView) -> ApiError {
    let code = err
        .error_prefix
        .unwrap_or_else(|| format!("rpc.error.{}", err.code));
    ApiError::Rejected(ApiErrorDetail {
        message: err.message,
        code,
    })
}

fn api_error_code(err: ApiError) -> String {
    match err {
        ApiError::InvalidArgument(detail)
        | ApiError::Rejected(detail)
        | ApiError::Internal(detail) => detail.code,
    }
}

fn nat_to_u128(value: &Nat) -> Option<u128> {
    let bytes = value.0.to_bytes_be();
    if bytes.len() > 16 {
        return None;
    }
    let mut out = [0u8; 16];
    out[16 - bytes.len()..].copy_from_slice(&bytes);
    Some(u128::from_be_bytes(out))
}

fn nat_to_fixed_be<const N: usize>(value: &Nat) -> Option<[u8; N]> {
    let bytes = value.0.to_bytes_be();
    if bytes.len() > N {
        return None;
    }
    let mut out = [0u8; N];
    out[N - bytes.len()..].copy_from_slice(&bytes);
    Some(out)
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
    validate_prune_policy_input(&policy)?;
    require_control_plane_write()?;
    let core_policy = evm_db::chain_data::PrunePolicy {
        target_bytes: policy.target_bytes,
        retain_days: policy.retain_days,
        retain_blocks: policy.retain_blocks,
        headroom_ratio_bps: policy.headroom_ratio_bps,
        hard_emergency_ratio_bps: policy.hard_emergency_ratio_bps,
        max_ops_per_tick: policy.max_ops_per_tick,
    };
    chain::set_prune_policy(core_policy).map_err(|_| "set_prune_policy failed".to_string())?;
    Ok(())
}

fn validate_prune_policy_input(policy: &PrunePolicyView) -> Result<(), String> {
    if policy.max_ops_per_tick < MIN_PRUNE_MAX_OPS_PER_TICK {
        return Err("input.prune.max_ops_per_tick.non_positive".to_string());
    }
    Ok(())
}

#[ic_cdk::update]
fn set_pruning_enabled(enabled: bool) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    chain::set_pruning_enabled(enabled).map_err(|_| "set_pruning_enabled failed".to_string())?;
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
fn rpc_eth_get_transaction_by_tx_id(tx_id: Vec<u8>) -> Option<EthTxView> {
    ic_evm_rpc::rpc_eth_get_transaction_by_tx_id(tx_id)
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_receipt_by_eth_hash(eth_tx_hash: Vec<u8>) -> Option<EthReceiptView> {
    ic_evm_rpc::rpc_eth_get_transaction_receipt_by_eth_hash(eth_tx_hash)
}

#[ic_cdk::query]
fn rpc_eth_get_balance(address: Vec<u8>, tag: RpcBlockTagView) -> Result<Vec<u8>, RpcErrorView> {
    ic_evm_rpc::rpc_eth_get_balance(address, tag)
}

#[ic_cdk::query]
fn rpc_eth_get_code(address: Vec<u8>, tag: RpcBlockTagView) -> Result<Vec<u8>, RpcErrorView> {
    ic_evm_rpc::rpc_eth_get_code(address, tag)
}

#[ic_cdk::query]
fn rpc_eth_get_storage_at(
    address: Vec<u8>,
    slot: Vec<u8>,
    tag: RpcBlockTagView,
) -> Result<Vec<u8>, RpcErrorView> {
    ic_evm_rpc::rpc_eth_get_storage_at(address, slot, tag)
}

#[ic_cdk::query]
fn rpc_eth_call_object(call: RpcCallObjectView) -> Result<RpcCallResultView, RpcErrorView> {
    ic_evm_rpc::rpc_eth_call_object(call)
}

#[cfg(feature = "precompile-profile-admin")]
#[ic_cdk::update]
fn profile_precompile_call(call: RpcCallObjectView) -> Result<RpcCallResultView, RpcErrorView> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(RpcErrorView {
            code: 1001,
            message: reason,
            error_prefix: Some("auth.controller_required".to_string()),
        });
    }
    if let Err(reason) = require_control_plane_write() {
        return Err(RpcErrorView {
            code: 1001,
            message: reason,
            error_prefix: Some("auth.controller_required".to_string()),
        });
    }
    ic_evm_rpc::rpc_eth_call_object(call)
}

#[ic_cdk::query]
fn rpc_eth_call_object_at(
    call: RpcCallObjectView,
    tag: RpcBlockTagView,
) -> Result<RpcCallResultView, RpcErrorView> {
    ic_evm_rpc::rpc_eth_call_object_at(call, tag)
}

#[ic_cdk::query]
fn rpc_eth_estimate_gas_object(call: RpcCallObjectView) -> Result<u64, RpcErrorView> {
    ic_evm_rpc::rpc_eth_estimate_gas_object(call)
}

#[ic_cdk::query]
fn rpc_eth_estimate_gas_object_at(
    call: RpcCallObjectView,
    tag: RpcBlockTagView,
) -> Result<u64, RpcErrorView> {
    ic_evm_rpc::rpc_eth_estimate_gas_object_at(call, tag)
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_count_at(
    address: Vec<u8>,
    tag: RpcBlockTagView,
) -> Result<u64, RpcErrorView> {
    ic_evm_rpc::rpc_eth_get_transaction_count_at(address, tag)
}

#[ic_cdk::query]
fn rpc_eth_max_priority_fee_per_gas() -> Result<candid::Nat, RpcErrorView> {
    ic_evm_rpc::rpc_eth_max_priority_fee_per_gas().map(candid::Nat::from)
}

#[ic_cdk::query]
fn rpc_eth_gas_price() -> Result<candid::Nat, RpcErrorView> {
    ic_evm_rpc::rpc_eth_gas_price().map(candid::Nat::from)
}

#[ic_cdk::query]
fn rpc_eth_fee_history(
    block_count: u64,
    newest: RpcBlockTagView,
    reward_percentiles: Option<Vec<f64>>,
) -> Result<RpcFeeHistoryView, RpcErrorView> {
    ic_evm_rpc::rpc_eth_fee_history(block_count, newest, reward_percentiles)
}

#[ic_cdk::query]
fn rpc_eth_history_window() -> RpcHistoryWindowView {
    ic_evm_rpc::rpc_eth_history_window()
}

#[ic_cdk::query]
fn rpc_eth_call_rawtx(raw_tx: Vec<u8>) -> Result<Vec<u8>, String> {
    ic_evm_rpc::rpc_eth_call_rawtx(raw_tx)
}

#[ic_cdk::query]
fn rpc_eth_get_logs_paged(
    filter: EthLogFilterView,
    cursor: Option<EthLogsCursorView>,
    limit: u32,
) -> Result<EthLogsPageView, GetLogsErrorView> {
    ic_evm_rpc::rpc_eth_get_logs_paged(filter, cursor, limit)
}

#[ic_cdk::query]
fn rpc_eth_get_block_by_number_with_status(number: u64, full_tx: bool) -> RpcBlockLookupView {
    ic_evm_rpc::rpc_eth_get_block_by_number_with_status(number, full_tx)
}

#[ic_cdk::query]
fn rpc_eth_get_block_number_by_hash(
    block_hash: Vec<u8>,
    max_scan: u32,
) -> Result<Option<u64>, String> {
    ic_evm_rpc::rpc_eth_get_block_number_by_hash(block_hash, max_scan)
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_receipt_with_status_by_eth_hash(
    eth_tx_hash: Vec<u8>,
) -> RpcReceiptLookupView {
    ic_evm_rpc::rpc_eth_get_transaction_receipt_with_status_by_eth_hash(eth_tx_hash)
}

#[ic_cdk::query]
fn rpc_eth_get_transaction_receipt_with_status_by_tx_id(tx_id: Vec<u8>) -> RpcReceiptLookupView {
    ic_evm_rpc::rpc_eth_get_transaction_receipt_with_status_by_tx_id(tx_id)
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
            // write-block条件と同じ判定を返し、運用上の見え方を一致させる。
            needs_migration: migration_pending(),
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
            query_instruction_soft_limit: state.chain_state.get().query_instruction_soft_limit,
            update_instruction_soft_limit: state.chain_state.get().update_instruction_soft_limit,
        }
    })
}

#[ic_cdk::update]
fn set_log_filter(filter: Option<String>) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
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
        if address.len() == 32 {
            return Err(
                "address must be 20 bytes (got 32; this looks like bytes32-encoded principal)"
                    .to_string(),
            );
        }
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
            auto_production_enabled: chain_state.auto_production_enabled,
            is_producing: chain_state.is_producing,
            mining_scheduled: chain_state.mining_scheduled,
            block_gas_limit: chain_state.block_gas_limit,
            query_instruction_soft_limit: chain_state.query_instruction_soft_limit,
            update_instruction_soft_limit: chain_state.update_instruction_soft_limit,
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
        }
    })
}

#[ic_cdk::query]
fn metrics_prometheus() -> Result<String, String> {
    let cycles = canister_cycle_balance();
    let stable_memory_pages = ic_cdk::stable::stable_size();
    let heap_memory_pages = current_heap_memory_pages();
    let now_nanos = current_time_nanos();
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
            auto_production_enabled: chain_state.auto_production_enabled,
            is_producing: chain_state.is_producing,
            mining_scheduled: chain_state.mining_scheduled,
            mining_interval_ms: DEFAULT_MINING_INTERVAL_MS,
            last_block_time: chain_state.last_block_time,
            pruned_before_block,
            drop_counts_by_code: metrics.drop_counts.to_vec(),
        })
    });
    ic_evm_metrics::encode_prometheus(now_nanos, &snapshot)
}

#[ic_cdk::query]
fn memory_breakdown() -> MemoryBreakdownView {
    let stable_pages_total = current_stable_memory_pages();
    let stable_bytes_total = stable_pages_total.saturating_mul(WASM_PAGE_SIZE_BYTES);
    let heap_pages = current_heap_memory_pages();
    let heap_bytes = heap_pages.saturating_mul(WASM_PAGE_SIZE_BYTES);
    let regions: Vec<MemoryRegionView> = all_memory_regions()
        .iter()
        .map(|region| {
            let pages = memory_size_pages(region.id);
            MemoryRegionView {
                id: region.id.as_u8(),
                name: region.name.to_string(),
                pages,
                bytes: pages.saturating_mul(WASM_PAGE_SIZE_BYTES),
            }
        })
        .collect();
    let regions_pages_total = regions
        .iter()
        .fold(0u64, |acc, r| acc.saturating_add(r.pages));
    let regions_bytes_total = regions
        .iter()
        .fold(0u64, |acc, r| acc.saturating_add(r.bytes));
    let unattributed_stable_pages = stable_pages_total.saturating_sub(regions_pages_total);
    let unattributed_stable_bytes = stable_bytes_total.saturating_sub(regions_bytes_total);
    MemoryBreakdownView {
        stable_pages_total,
        stable_bytes_total,
        regions_pages_total,
        regions_bytes_total,
        unattributed_stable_pages,
        unattributed_stable_bytes,
        heap_pages,
        heap_bytes,
        regions,
    }
}

fn apply_instruction_soft_limits_from_init_args(args: &InitArgs) {
    if args.query_instruction_soft_limit.is_none() && args.update_instruction_soft_limit.is_none() {
        return;
    }
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        if let Some(limit) = args.query_instruction_soft_limit {
            chain_state.query_instruction_soft_limit = limit;
        }
        if let Some(limit) = args.update_instruction_soft_limit {
            chain_state.update_instruction_soft_limit = limit;
        }
        state.chain_state.set(chain_state);
    });
}

#[cfg(feature = "precompile-profile-admin")]
#[ic_cdk::query]
fn get_precompile_profile() -> Vec<PrecompileProfileView> {
    if let Err(reason) = require_controller() {
        ic_cdk::trap(&reason);
    }
    evm_core::wrap_precompile::precompile_profile_snapshot()
        .into_iter()
        .map(|entry| PrecompileProfileView {
            address: entry.address.to_vec(),
            calls: entry.calls,
            total_instructions: entry.total_instructions,
            avg_instructions: entry.avg_instructions,
            max_instructions: entry.max_instructions,
            total_extra_gas: entry.total_extra_gas,
            avg_extra_gas: entry.avg_extra_gas,
            max_extra_gas: entry.max_extra_gas,
        })
        .collect()
}

#[cfg(feature = "precompile-profile-admin")]
#[ic_cdk::update]
fn clear_precompile_profile() -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    evm_core::wrap_precompile::clear_precompile_profile();
    Ok(())
}

#[ic_cdk::update]
fn prune_blocks(retain: u64, max_ops: u32) -> Result<PruneResultView, ProduceBlockError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(ProduceBlockError::Internal(reason));
    }
    if let Err(reason) = require_control_plane_write() {
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

#[ic_cdk::query]
fn get_unwrap_dispatch_overview(request_id: Vec<u8>) -> Option<UnwrapDispatchOverviewView> {
    let request_id = request_id_from_bytes(request_id)?;
    with_state(|state| {
        state
            .unwrap_requests
            .get(&TxId(request_id))
            .map(|value| UnwrapDispatchOverviewView {
                request_id: request_id.to_vec(),
                status: request_dispatch_status_to_view(value.status),
                error: normalize_unwrap_error_code_for_view(value.error_code.as_deref()),
            })
    })
}

#[ic_cdk::query]
fn get_unwrap_request_ids_by_tx_id(tx_id: Vec<u8>) -> Vec<Vec<u8>> {
    let Some(tx_id) = tx_id_from_bytes(tx_id) else {
        return Vec::new();
    };
    unwrap_request_ids_for_tx(&tx_id)
        .into_iter()
        .map(|request_id| request_id.0.to_vec())
        .collect()
}

fn normalize_unwrap_error_code_for_view(error_code: Option<&str>) -> Option<String> {
    match error_code {
        Some(UNWRAP_DECODE_FAILURE_CODE) => Some(UNWRAP_QUARANTINE_ERROR.to_string()),
        Some(value) => Some(value.to_string()),
        None => None,
    }
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

#[cfg(target_arch = "wasm32")]
fn current_stable_memory_pages() -> u64 {
    ic_cdk::stable::stable_size()
}

#[cfg(not(target_arch = "wasm32"))]
fn current_stable_memory_pages() -> u64 {
    all_memory_regions().iter().fold(0u64, |acc, region| {
        acc.saturating_add(memory_size_pages(region.id))
    })
}

fn tx_id_from_bytes(tx_id: Vec<u8>) -> Option<TxId> {
    if tx_id.len() != 32 {
        return None;
    }
    let mut buf = [0u8; 32];
    buf.copy_from_slice(&tx_id);
    Some(TxId(buf))
}

fn require_controller() -> Result<(), String> {
    let caller = msg_caller();
    if !is_controller(&caller) {
        return Err("auth.controller_required".to_string());
    }
    Ok(())
}

// 制御プレーン（管理API）は非常時でも controller 操作を継続できるよう、
// reject_write_reason には依存させない。
fn require_control_plane_write() -> Result<(), String> {
    require_controller()?;
    Ok(())
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
                if state.from_version < 5 {
                    chain::clear_eth_tx_hash_index();
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
                if state.from_version < 5 {
                    let start_key = if state.cursor_key_set {
                        Some(TxId(state.cursor_key))
                    } else {
                        None
                    };
                    let (last_key, rebuilt, done) =
                        chain::rebuild_eth_tx_hash_index_batch(start_key, 512);
                    state.cursor = state.cursor.saturating_add(rebuilt);
                    if let Some(key) = last_key {
                        state.cursor_key_set = true;
                        state.cursor_key = key.0;
                    }
                    set_schema_migration_state(state);
                    if !done {
                        return false;
                    }
                    state.cursor_key_set = false;
                    state.cursor_key = [0u8; 32];
                    set_schema_migration_state(state);
                }
                state.phase = SchemaMigrationPhase::Verify;
                state.cursor = 0;
                set_schema_migration_state(state);
            }
            SchemaMigrationPhase::Verify => {
                if !evm_db::meta::tx_locs_v3_active() {
                    state.phase = SchemaMigrationPhase::Error;
                    state.last_error = 2;
                    set_schema_migration_state(state);
                    return false;
                }
                if state.from_version < 5 {
                    let (index_ok, indexed, expected) = chain::verify_eth_tx_hash_index(256);
                    if !index_ok {
                        warn!(
                            indexed_eth_hashes = indexed,
                            expected_eth_signed = expected,
                            "eth_tx_hash index verification failed"
                        );
                        state.phase = SchemaMigrationPhase::Error;
                        state.last_error = 4;
                        set_schema_migration_state(state);
                        return false;
                    }
                }
                mark_migration_applied(state.from_version, state.to_version, current_time_nanos());
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
    let now = current_time_nanos();
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
    if from < current {
        sync_chain_runtime_defaults_for_schema_upgrade();
    }
    if from >= 3 && !evm_db::meta::tx_locs_v3_active() {
        set_tx_locs_v3_active(true);
    }
    let state_root_pending = with_state(|state| {
        !state.state_root_meta.get().initialized
            || state.state_root_migration.get().phase != MigrationPhase::Done
    });
    if from < current || meta.needs_migration || state_root_pending {
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

fn sync_chain_runtime_defaults_for_schema_upgrade() {
    evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        // 運用パラメータの stale state を upgrade で矯正する。
        // base_fee は市場状態として維持し、gas limit と fee floor だけ既定値へ戻す。
        chain_state.block_gas_limit = DEFAULT_BLOCK_GAS_LIMIT;
        chain_state.min_gas_price = DEFAULT_MIN_FEE_FLOOR;
        chain_state.min_priority_fee = DEFAULT_MIN_FEE_FLOOR;
        state.chain_state.set(chain_state);
    });
}

fn schedule_cycle_observer() {
    // migration中は短周期、通常時は1時間周期で再スケジュールする。
    let interval_secs = if migration_pending() {
        CYCLE_OBSERVER_FAST_INTERVAL_SECS
    } else {
        CYCLE_OBSERVER_SLOW_INTERVAL_SECS
    };
    ic_cdk_timers::set_timer(std::time::Duration::from_secs(interval_secs), async move {
        let _ = run_cycle_observer_once();
        schedule_cycle_observer();
    });
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CycleObserverTickOutcome {
    migration_tick_ran: bool,
    migration_pending: bool,
    mode: OpsMode,
    schedule_mining_called: bool,
}

fn run_cycle_observer_once() -> CycleObserverTickOutcome {
    let mut migration_tick_ran = false;
    if should_run_cycle_observer_migration_tick(migration_pending()) {
        drive_migrations_tick(256, 512);
        migration_tick_ran = true;
    }
    let migration_pending = migration_pending();
    let mode = observe_cycles();
    let schedule_mining_called =
        should_schedule_mining_after_cycle_observer(mode, migration_pending);
    if schedule_mining_called {
        schedule_mining();
    }
    info!(
        event = "cycle_observer_tick",
        migration_pending,
        mode = ?mode,
        schedule_mining_called
    );
    CycleObserverTickOutcome {
        migration_tick_ran,
        migration_pending,
        mode,
        schedule_mining_called,
    }
}

fn should_run_cycle_observer_migration_tick(migration_pending: bool) -> bool {
    migration_pending
}

fn should_schedule_mining_after_cycle_observer(mode: OpsMode, migration_pending: bool) -> bool {
    mode != OpsMode::Critical && !migration_pending
}

fn schedule_mining() {
    schedule_mining_with_timer(install_mining_timer, reject_write_reason);
}

fn schedule_mining_with_timer(timer_scheduler: fn(u64), reject_provider: fn() -> Option<String>) {
    if reject_provider().is_some() {
        return;
    }
    // RefCell再入防止: with_state_mut内は状態更新のみ。タイマー副作用は借用解放後に実行する。
    let interval_ms = evm_db::stable_state::with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        if !chain_state.auto_production_enabled {
            return None;
        }
        if chain_state.mining_scheduled {
            return None;
        }
        chain_state.mining_scheduled = true;
        state.chain_state.set(chain_state);
        Some(DEFAULT_MINING_INTERVAL_MS)
    });
    if let Some(interval_ms) = interval_ms {
        timer_scheduler(interval_ms);
    }
}

fn install_mining_timer(interval_ms: u64) {
    ic_cdk_timers::set_timer(std::time::Duration::from_millis(interval_ms), async move {
        mining_tick();
    });
}

fn should_prune_on_block_event(block_number: u64) -> bool {
    block_number != 0 && block_number.is_multiple_of(PRUNE_EVENT_BLOCK_INTERVAL)
}

fn maybe_prune_on_block_event(block_number: u64) {
    if !should_prune_on_block_event(block_number) {
        return;
    }
    if let Err(err) = chain::prune_tick() {
        PRUNE_ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
        error!(error = ?err, block_number, "prune_tick failed on block event");
    }
}

fn mining_tick() {
    mining_tick_with_timer(install_mining_timer, reject_write_reason);
}

fn mining_tick_with_timer(timer_scheduler: fn(u64), reject_provider: fn() -> Option<String>) {
    if reject_provider().is_some() {
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
        if !chain_state.auto_production_enabled {
            state.chain_state.set(chain_state);
            return false;
        }
        if chain_state.is_producing {
            state.chain_state.set(chain_state);
            return false;
        }
        if state.ready_queue.is_empty() {
            state.chain_state.set(chain_state);
            return false;
        }
        chain_state.is_producing = true;
        state.chain_state.set(chain_state);
        true
    });

    if should_produce {
        let result = chain::produce_block(evm_db::chain_data::MAX_TXS_PER_BLOCK);

        evm_db::stable_state::with_state_mut(|state| {
            let mut chain_state = *state.chain_state.get();
            chain_state.is_producing = false;
            state.chain_state.set(chain_state);
        });
        match result {
            Ok(outcome) => {
                record_unwrap_requests_from_block(&outcome.block.tx_ids);
                schedule_unwrap_dispatch();
                maybe_prune_on_block_event(outcome.block.number);
            }
            Err(chain::ChainError::NoExecutableTx) | Err(chain::ChainError::QueueEmpty) => {}
            Err(err) => {
                MINING_ERROR_COUNT.fetch_add(1, Ordering::Relaxed);
                error!(error = ?err, "mining_tick produce_block failed");
            }
        }
        let has_ready_tx = with_state(|state| !state.ready_queue.is_empty());
        if has_ready_tx {
            schedule_mining_with_timer(timer_scheduler, reject_provider);
        }
    }
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct WrapSubmitUnwrapRequestArgs {
    request_id: Vec<u8>,
    asset_id: Principal,
    amount_e8s: Nat,
    recipient: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct WrapSubmitUnwrapRequestOk {
    request_id: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum WrapSubmitDispatchOutcome {
    Accepted,
    Rejected(String),
    RequestIdMismatch,
    DecodeFailed(String),
    CallFailed(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppliedUnwrapDispatchOutcome {
    status: UnwrapRequestStatus,
    error_code: Option<String>,
}

fn record_unwrap_requests_from_block(tx_ids: &[TxId]) {
    for tx_id in tx_ids {
        let Some(receipt) = chain::get_receipt(tx_id) else {
            continue;
        };
        for (log_index, log) in receipt.logs.iter().enumerate() {
            let Some(intent) = unwrap_intent_from_log(log) else {
                continue;
            };
            let Some(request_id) = derive_unwrap_request_id(tx_id, log_index) else {
                continue;
            };
            with_state_mut(|state| {
                if state.unwrap_requests.get(&request_id).is_some() {
                    return;
                }
                let now = current_time_nanos();
                state.unwrap_requests.insert(
                    request_id,
                    UnwrapDispatchRequest {
                        asset_id: intent.asset_id.clone(),
                        amount: intent.amount,
                        recipient: intent.recipient.clone(),
                        status: UnwrapRequestStatus::Queued,
                        ledger_tx_id: None,
                        error_code: None,
                        updated_at: now,
                    },
                );
                let mut meta = *state.unwrap_dispatch_meta.get();
                let seq = meta.push();
                state.unwrap_dispatch_meta.set(meta);
                state.unwrap_dispatch_queue.insert(seq, request_id);
            });
        }
    }
}

fn schedule_unwrap_dispatch() {
    let should_schedule = !UNWRAP_DISPATCH_SCHEDULED.swap(true, Ordering::SeqCst);
    if !should_schedule {
        return;
    }
    ic_cdk_timers::set_timer(
        std::time::Duration::from_millis(WRAP_DISPATCH_DELAY_MS),
        async move {
            unwrap_dispatch_tick().await;
        },
    );
}

fn pop_next_dispatch_request(now: u64) -> Result<Option<(TxId, UnwrapDispatchRequest)>, String> {
    let out = with_state_mut(|state| {
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = match meta.pop() {
            Some(v) => v,
            None => {
                state.unwrap_dispatch_meta.set(meta);
                return Ok(None);
            }
        };
        state.unwrap_dispatch_meta.set(meta);

        let Some(request_id) = state.unwrap_dispatch_queue.get(&seq) else {
            return Err(format!("wrap.dispatch.queue_missing:seq={seq}"));
        };
        state.unwrap_dispatch_queue.remove(&seq);
        let Some(mut req) = state.unwrap_requests.get(&request_id) else {
            return Err(format!(
                "wrap.dispatch.request_missing:request_id={:?}",
                request_id.0
            ));
        };
        if should_skip_dispatch_unwrap_request(&req) {
            req.updated_at = now;
            req.status = UnwrapRequestStatus::DispatchFailed;
            req.ledger_tx_id = None;
            req.error_code = Some(UNWRAP_QUARANTINE_ERROR.to_string());
            state.unwrap_requests.insert(request_id, req);
            return Err(format!(
                "wrap.dispatch.quarantined:request_id={:?}:reason={UNWRAP_QUARANTINE_ERROR}",
                request_id.0
            ));
        }
        req.status = UnwrapRequestStatus::Dispatching;
        req.updated_at = now;
        state.unwrap_requests.insert(request_id, req.clone());
        Ok(Some((request_id, req)))
    });
    out
}

fn is_decode_failed_unwrap_request(req: &UnwrapDispatchRequest) -> bool {
    req.error_code.as_deref() == Some(UNWRAP_DECODE_FAILURE_CODE)
}

fn is_quarantined_unwrap_request(req: &UnwrapDispatchRequest) -> bool {
    req.error_code.as_deref() == Some(UNWRAP_QUARANTINE_ERROR)
}

fn should_skip_dispatch_unwrap_request(req: &UnwrapDispatchRequest) -> bool {
    is_decode_failed_unwrap_request(req) || is_quarantined_unwrap_request(req)
}

fn quarantine_decode_failed_unwrap_requests(now: u64) -> (u64, u64) {
    let out = with_state_mut(|state| {
        let mut request_ids = Vec::new();
        for entry in state.unwrap_requests.iter() {
            if is_decode_failed_unwrap_request(&entry.value()) {
                request_ids.push(*entry.key());
            }
        }
        for request_id in request_ids.iter().copied() {
            let Some(mut req) = state.unwrap_requests.get(&request_id) else {
                continue;
            };
            req.updated_at = now;
            req.status = UnwrapRequestStatus::DispatchFailed;
            req.ledger_tx_id = None;
            req.error_code = Some(UNWRAP_QUARANTINE_ERROR.to_string());
            state.unwrap_requests.insert(request_id, req);
        }
        if request_ids.is_empty() {
            return (0, 0);
        }
        let request_ids_set: std::collections::BTreeSet<TxId> =
            request_ids.iter().copied().collect();
        let mut dispatch_seq_to_drop = Vec::new();
        for entry in state.unwrap_dispatch_queue.iter() {
            let queued_request_id = entry.value();
            if request_ids_set.contains(&queued_request_id) {
                dispatch_seq_to_drop.push(*entry.key());
            }
        }
        for seq in &dispatch_seq_to_drop {
            state.unwrap_dispatch_queue.remove(seq);
        }
        (
            request_ids.len() as u64,
            u64::try_from(dispatch_seq_to_drop.len()).unwrap_or(0),
        )
    });
    out
}

async fn unwrap_dispatch_tick() {
    UNWRAP_DISPATCH_SCHEDULED.store(false, Ordering::SeqCst);
    loop {
        let next = pop_next_dispatch_request(current_time_nanos());
        let Some((request_id, req)) = (match next {
            Ok(v) => v,
            Err(err) => {
                if err.starts_with("wrap.dispatch.quarantined:") {
                    warn!(error = err, "unwrap_dispatch_tick quarantined request");
                } else {
                    error!(
                        error = err,
                        "unwrap_dispatch_tick skipped corrupted queue entry"
                    );
                }
                continue;
            }
        }) else {
            break;
        };

        let args = match build_wrap_submit_unwrap_request_args(&request_id, &req) {
            Ok(args) => args,
            Err(code) => {
                finalize_unwrap_dispatch_attempt(
                    request_id,
                    current_time_nanos(),
                    AppliedUnwrapDispatchOutcome {
                        status: UnwrapRequestStatus::DispatchFailed,
                        error_code: Some(code),
                    },
                );
                if with_state(|state| !state.unwrap_dispatch_queue.is_empty()) {
                    schedule_unwrap_dispatch();
                }
                break;
            }
        };
        let submit = ic_cdk::call::Call::unbounded_wait(
            current_wrap_canister_id(),
            "dispatch_unwrap_request",
        )
        .with_arg(args)
        .await;

        finalize_unwrap_dispatch_attempt(
            request_id,
            current_time_nanos(),
            apply_unwrap_dispatch_outcome(resolve_wrap_submit_outcome(&request_id.0, submit)),
        );

        if with_state(|state| !state.unwrap_dispatch_queue.is_empty()) {
            schedule_unwrap_dispatch();
        }
        break;
    }
}

fn build_wrap_submit_unwrap_request_args(
    request_id: &TxId,
    req: &UnwrapDispatchRequest,
) -> Result<WrapSubmitUnwrapRequestArgs, String> {
    Ok(WrapSubmitUnwrapRequestArgs {
        request_id: request_id.0.to_vec(),
        asset_id: Principal::from_slice(req.asset_id.as_slice()),
        amount_e8s: Nat(BigUint::from_bytes_be(req.amount.as_slice())),
        recipient: Principal::from_slice(req.recipient.as_slice()),
    })
}

fn resolve_wrap_submit_outcome(
    expected_request_id: &[u8; 32],
    submit: Result<ic_cdk::call::Response, ic_cdk::call::CallFailed>,
) -> WrapSubmitDispatchOutcome {
    match submit {
        Ok(resp) => match resp.candid_tuple::<(Result<WrapSubmitUnwrapRequestOk, ApiError>,)>() {
            Ok((Ok(ok),)) => resolve_wrap_submit_ok(expected_request_id, &ok),
            Ok((Err(err),)) => WrapSubmitDispatchOutcome::Rejected(api_error_code(err)),
            Err(err) => WrapSubmitDispatchOutcome::DecodeFailed(err.to_string()),
        },
        Err(err) => WrapSubmitDispatchOutcome::CallFailed(err.to_string()),
    }
}

fn resolve_wrap_submit_ok(
    expected_request_id: &[u8; 32],
    ok: &WrapSubmitUnwrapRequestOk,
) -> WrapSubmitDispatchOutcome {
    if ok.request_id.as_slice() == expected_request_id {
        WrapSubmitDispatchOutcome::Accepted
    } else {
        WrapSubmitDispatchOutcome::RequestIdMismatch
    }
}

fn apply_unwrap_dispatch_outcome(
    outcome: WrapSubmitDispatchOutcome,
) -> AppliedUnwrapDispatchOutcome {
    match outcome {
        WrapSubmitDispatchOutcome::Accepted => AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::Dispatched,
            error_code: None,
        },
        WrapSubmitDispatchOutcome::Rejected(code) => AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::DispatchFailed,
            error_code: Some(format!("wrap.submit_failed:{code}")),
        },
        WrapSubmitDispatchOutcome::RequestIdMismatch => AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::DispatchFailed,
            error_code: Some("wrap.request_id_mismatch".to_string()),
        },
        WrapSubmitDispatchOutcome::DecodeFailed(err) => AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::DispatchFailed,
            error_code: Some(format!("wrap.decode_failed:{err}")),
        },
        WrapSubmitDispatchOutcome::CallFailed(err) => AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::DispatchFailed,
            error_code: Some(format!("wrap.call_failed:{err}")),
        },
    }
}

fn finalize_unwrap_dispatch_attempt(
    request_id: TxId,
    now: u64,
    applied: AppliedUnwrapDispatchOutcome,
) {
    with_state_mut(|state| {
        let Some(mut req) = state.unwrap_requests.get(&request_id) else {
            return;
        };
        req.updated_at = now;
        req.ledger_tx_id = None;
        req.status = applied.status;
        req.error_code = applied.error_code;
        state.unwrap_requests.insert(request_id, req);
    });
}

fn request_id_from_bytes(bytes: Vec<u8>) -> Option<[u8; 32]> {
    if bytes.len() != 32 {
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

fn derive_unwrap_request_id(tx_id: &TxId, log_index: usize) -> Option<TxId> {
    let log_index = u32::try_from(log_index).ok()?;
    let mut payload = Vec::with_capacity(36);
    payload.extend_from_slice(&tx_id.0);
    payload.extend_from_slice(&log_index.to_be_bytes());
    Some(TxId(hash::keccak256(&payload)))
}

fn unwrap_request_ids_for_tx(tx_id: &TxId) -> Vec<TxId> {
    let Some(receipt) = chain::get_receipt(tx_id) else {
        return Vec::new();
    };
    receipt
        .logs
        .iter()
        .enumerate()
        .filter_map(|(log_index, log)| {
            unwrap_intent_from_log(log)?;
            derive_unwrap_request_id(tx_id, log_index)
        })
        .collect()
}

fn request_dispatch_status_to_view(status: UnwrapRequestStatus) -> RequestDispatchStatusView {
    match status {
        UnwrapRequestStatus::Queued => RequestDispatchStatusView::Queued,
        UnwrapRequestStatus::Dispatching => RequestDispatchStatusView::Dispatching,
        UnwrapRequestStatus::Dispatched => RequestDispatchStatusView::Dispatched,
        UnwrapRequestStatus::DispatchFailed => RequestDispatchStatusView::DispatchFailed,
    }
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
mod tests;
