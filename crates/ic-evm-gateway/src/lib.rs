//! どこで: canister入口 / 何を: Phase1のAPI公開 / なぜ: submit中心の安全な運用導線を提供するため

use candid::{CandidType, Nat, Principal};
use evm_core::chain;
use evm_core::hash;
use evm_core::kasane_precompiles::{
    icp_update_intent_from_log, native_withdraw_intent_from_log, precompile_allow_key,
    unwrap_intent_from_log,
};
use evm_core::tx_decode::decode_tx_view;
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::chain_data::constants::{MAX_QUEUE_SNAPSHOT_LIMIT, MAX_RETURN_DATA, MAX_TX_SIZE};
use evm_db::chain_data::runtime_defaults::{DEFAULT_BLOCK_GAS_LIMIT, DEFAULT_MIN_FEE_FLOOR};
use evm_db::chain_data::DEFAULT_MINING_INTERVAL_MS;
use evm_db::chain_data::MIN_PRUNE_MAX_OPS_PER_TICK;
use evm_db::chain_data::{
    BlockData, FeePolicyStored, IcpUpdateDispatchRequest, IcpUpdateRequestStatus, MigrationPhase,
    MintSubmitStatus, OpsMode, ReceiptLike, RequestStatus as StoredRequestStatus, RuntimeConfigV1,
    TxId, TxKind, TxLoc, TxLocKind, UnwrapDispatchRequest, UnwrapRequestStatus,
    WrapEvmConfigStored, WrapPendingSubmission, WrapRequestStage, ICP_UPDATE_DECODE_FAILURE_CODE,
    LOG_CONFIG_FILTER_MAX, UNWRAP_DECODE_FAILURE_CODE,
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
use ic_cdk::call::{Call, CallFailed, RejectCode};
use num_bigint::BigUint;
use serde::Deserialize;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use tiny_keccak::{Hasher, Keccak};
use tracing::{error, info, warn};

mod icrc21;

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
const NATIVE_WITHDRAW_ASSET_MARKER: &[u8] = b"kasane.native.icp";
const MAX_CYCLE_FEE_E8S: u64 = 1_000_000_000_000;
const GAS_PRICE_DENOMINATOR_BPS: u128 = 10_000;
const WEI_PER_E8S: u128 = 10_000_000_000;
const MAX_STORED_ERROR_CODE_BYTES: usize = 160;
const MAX_STORED_LEDGER_TX_ID_BYTES: usize = 128;
const STALE_OPERATION_NANOS: u64 = 10 * 60 * 1_000_000_000;
const ICP_UPDATE_REPLY_OMITTED_TOO_LARGE: &str = "ic_update.reply_omitted_too_large";
const ICP_UPDATE_DISPATCH_TIMEOUT_SECONDS: u32 = 30;
const MAX_ICP_UPDATE_REQUESTS: usize = 10_000;

static UNWRAP_DISPATCH_SCHEDULED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static ICP_UPDATE_DISPATCH_SCHEDULED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
#[cfg(test)]
static ICP_UPDATE_DISPATCH_TIMER_ARMS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
static WRAP_WORKER_SCHEDULED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

use ic_evm_rpc_types::*;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub genesis_balances: Vec<GenesisBalanceView>,
    pub wrap_canister_id: Principal,
    pub wrap_factory_address: Vec<u8>,
    pub wrap_config: Option<WrapConfigArgs>,
    pub query_instruction_soft_limit: Option<u64>,
    pub update_instruction_soft_limit: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WrapConfigArgs {
    pub fee_ledger_canister: Principal,
    pub native_ledger_canister: Principal,
    pub cycle_fee_e8s: u64,
    pub gas_price_buffer_bps: u32,
    pub allowed_assets: Vec<Principal>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct FeePolicyView {
    pub fee_ledger_canister: Principal,
    pub cycle_fee_e8s: u64,
    pub gas_price_buffer_bps: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SetFeePolicyArgs {
    pub fee_ledger_canister: Principal,
    pub cycle_fee_e8s: u64,
    pub gas_price_buffer_bps: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct PrecompileAllowArgs {
    pub target: Principal,
    pub method: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct PrecompileAllowedView {
    pub target: Principal,
    pub method: String,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct IcpUpdateRequestView {
    pub request_id: Vec<u8>,
    pub tx_id: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub log_index: u32,
    pub tx_kind: IcpUpdateTxKindView,
    pub evm_sender: Vec<u8>,
    pub ic_caller: Option<Principal>,
    pub target: Principal,
    pub method: String,
    pub status: RequestDispatchStatusView,
    pub reply: Option<Vec<u8>>,
    pub error: Option<String>,
    pub updated_at: u64,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum IcpUpdateTxKindView {
    EthSigned,
    IcSynthetic,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct IcpUpdateEnvelopeV1 {
    pub version: u8,
    pub chain_id: u64,
    pub request_id: Vec<u8>,
    pub tx_id: Vec<u8>,
    pub block_number: u64,
    pub tx_index: u32,
    pub log_index: u32,
    pub tx_kind: IcpUpdateTxKindView,
    pub evm_sender: Vec<u8>,
    pub ic_caller: Option<Principal>,
    pub arg: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct FeePolicyArgs {
    pub fee_ledger_canister: Principal,
    pub cycle_fee_e8s: u64,
    pub gas_price_buffer_bps: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QuoteWrapRequestArgs {
    pub asset_id: Principal,
    pub amount_e8s: Nat,
    pub evm_recipient: Vec<u8>,
    pub gas_limit: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QuoteWrapRequestOk {
    pub charged_fee_e8s: Nat,
    pub charged_gas_price_wei: Nat,
    pub cycle_fee_e8s: u64,
    pub fee_ledger_canister: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SubmitWrapRequestArgs {
    pub asset_id: Principal,
    pub amount_e8s: Nat,
    pub evm_recipient: Vec<u8>,
    pub evm_nonce: u64,
    pub gas_limit: u64,
    pub max_fee_e8s: Nat,
    pub quoted_gas_price_wei: Nat,
    pub fee_ledger_canister: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SubmitWrapRequestOk {
    pub request_id: Vec<u8>,
    pub charged_fee_e8s: Nat,
    pub charged_gas_price_wei: Nat,
    pub fee_ledger_tx_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct GetUnwrapRequirementsArgs {
    pub asset_id: Principal,
    pub amount_e8s: Nat,
    pub caller_evm_address: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct GetUnwrapRequirementsOk {
    pub factory_address: Vec<u8>,
    pub wrapped_token_address: Option<Vec<u8>>,
    pub balance: Nat,
    pub allowance: Nat,
    pub approve_required: bool,
    pub readiness: UnwrapReadiness,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum UnwrapReadiness {
    Ready,
    TokenNotDeployed,
    InsufficientBalance,
    InsufficientAllowance,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QuoteNativeDepositArgs {
    pub amount_e8s: Nat,
    pub evm_recipient: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QuoteNativeDepositOk {
    pub charged_fee_e8s: Nat,
    pub native_ledger_canister: Principal,
    pub fee_ledger_canister: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QuoteNativeWithdrawalArgs {
    pub amount_e8s: Nat,
    pub recipient: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct QuoteNativeWithdrawalOk {
    pub native_ledger_canister: Principal,
    pub ledger_fee_e8s: Nat,
    pub receive_amount_e8s: Nat,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WrapRuntimeConfigView {
    pub native_ledger_canister: Principal,
    pub fee_ledger_canister: Principal,
    pub allowed_assets: Vec<Principal>,
    pub wrap_factory_address: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SubmitNativeDepositArgs {
    pub deposit_id: Vec<u8>,
    pub amount_e8s: Nat,
    pub evm_recipient: Vec<u8>,
    pub max_fee_e8s: Nat,
    pub fee_ledger_canister: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SubmitNativeDepositOk {
    pub request_id: Vec<u8>,
    pub charged_fee_e8s: Nat,
    pub fee_ledger_tx_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DispatchUnwrapRequestArgs {
    pub request_id: Vec<u8>,
    pub asset_id: Principal,
    pub amount_e8s: Nat,
    pub recipient: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DispatchNativeWithdrawalRequestArgs {
    pub request_id: Vec<u8>,
    pub amount_e8s: Nat,
    pub recipient: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DispatchUnwrapRequestOk {
    pub request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct RetryRequestArgs {
    pub request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct RecoverFailedWrapArgs {
    pub request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Icrc1Account {
    owner: Principal,
    subaccount: Option<Vec<u8>>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Icrc1TransferArg {
    from_subaccount: Option<Vec<u8>>,
    to: Icrc1Account,
    amount: Nat,
    fee: Option<Nat>,
    memo: Option<Vec<u8>>,
    created_at_time: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum Icrc1TransferError {
    BadFee { expected_fee: Nat },
    BadBurn { min_burn_amount: Nat },
    InsufficientFunds { balance: Nat },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    TemporarilyUnavailable,
    Duplicate { duplicate_of: Nat },
    GenericError { error_code: Nat, message: String },
}

#[derive(Clone, Debug, CandidType, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum Icrc1MetadataValue {
    Int(candid::Int),
    Nat(Nat),
    Blob(Vec<u8>),
    Text(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct Icrc2TransferFromArg {
    from: Icrc1Account,
    spender_subaccount: Option<Vec<u8>>,
    to: Icrc1Account,
    amount: Nat,
    fee: Option<Nat>,
    memo: Option<Vec<u8>>,
    created_at_time: Option<u64>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum Icrc2TransferFromError {
    BadFee { expected_fee: Nat },
    BadBurn { min_burn_amount: Nat },
    InsufficientFunds { balance: Nat },
    InsufficientAllowance { allowance: Nat },
    TooOld,
    CreatedInFuture { ledger_time: u64 },
    TemporarilyUnavailable,
    Duplicate { duplicate_of: Nat },
    GenericError { error_code: Nat, message: String },
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum RequestKind {
    Wrap,
    NativeDeposit,
    Unwrap,
    NativeWithdrawal,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum RequestStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl PartialEq<StoredRequestStatus> for RequestStatus {
    fn eq(&self, other: &StoredRequestStatus) -> bool {
        matches!(
            (self, other),
            (RequestStatus::Queued, StoredRequestStatus::Queued)
                | (RequestStatus::Running, StoredRequestStatus::Running)
                | (RequestStatus::Succeeded, StoredRequestStatus::Succeeded)
                | (RequestStatus::Failed, StoredRequestStatus::Failed)
        )
    }
}

fn request_status_to_view(status: StoredRequestStatus) -> RequestStatus {
    match status {
        StoredRequestStatus::Queued => RequestStatus::Queued,
        StoredRequestStatus::Running => RequestStatus::Running,
        StoredRequestStatus::Succeeded => RequestStatus::Succeeded,
        StoredRequestStatus::Failed => RequestStatus::Failed,
    }
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct RequestErrorView {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum RequestStageView {
    FeePending,
    FeeCollected,
    PullPending,
    Pulled,
    MintSubmitting,
    MintSubmitted,
    Succeeded,
    Failed,
    Refunding,
    Refunded,
    Queued,
    Dispatching,
    Dispatched,
    DispatchFailed,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct RequestOverview {
    pub kind: RequestKind,
    pub request_id: Vec<u8>,
    pub status: RequestStatus,
    pub stage: Option<RequestStageView>,
    pub error: Option<RequestErrorView>,
    pub fee_ledger_tx_id: Option<Vec<u8>>,
    pub pull_ledger_tx_id: Option<Vec<u8>>,
    pub mint_tx_id: Option<Vec<u8>>,
    pub withdraw_ledger_tx_id: Option<Vec<u8>>,
    pub recoverable: bool,
    pub withdrawn: bool,
    pub withdraw_in_progress: bool,
    pub withdraw_error: Option<RequestErrorView>,
    pub ledger_tx_id: Option<Vec<u8>>,
    pub dispatch_status: Option<RequestDispatchStatusView>,
    pub dispatch_error: Option<String>,
    pub charged_fee_e8s: Option<Nat>,
    pub charged_gas_price_wei: Option<Nat>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransferMemoKind {
    Unwrap,
    Fee,
    Pull,
    Withdraw,
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
        if let Some(config) = &self.wrap_config {
            validate_wrap_config(config)?;
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
        RuntimeConfigV1::try_new_from_bytes(
            runtime_wrap_canister_id(self.wrap_canister_id).as_slice(),
            wrap_factory_address,
        )
        .unwrap_or_else(|err| ic_cdk::trap(format!("InvalidRuntimeConfig: {err}")))
    }
}

fn validate_wrap_config(config: &WrapConfigArgs) -> Result<(), String> {
    validate_non_anonymous_principal(&config.fee_ledger_canister, "wrap.fee_ledger_anonymous")?;
    validate_non_anonymous_principal(
        &config.native_ledger_canister,
        "wrap.native_ledger_anonymous",
    )?;
    if config.cycle_fee_e8s > MAX_CYCLE_FEE_E8S {
        return Err("wrap.cycle_fee_e8s_out_of_range".to_string());
    }
    if !(10_000..=50_000).contains(&config.gas_price_buffer_bps) {
        return Err("wrap.gas_price_buffer_bps_out_of_range".to_string());
    }
    if config.allowed_assets.is_empty() {
        return Err("wrap.allowed_assets_empty".to_string());
    }
    for asset in &config.allowed_assets {
        validate_non_anonymous_principal(asset, "wrap.allowed_asset_anonymous")?;
        if *asset == config.native_ledger_canister {
            return Err("wrap.native_ledger_not_wrappable".to_string());
        }
    }
    Ok(())
}

fn validate_set_fee_policy(args: &FeePolicyArgs) -> Result<(), String> {
    validate_non_anonymous_principal(&args.fee_ledger_canister, "arg.fee_ledger_anonymous")?;
    if args.cycle_fee_e8s > MAX_CYCLE_FEE_E8S {
        return Err("arg.cycle_fee_e8s_out_of_range".to_string());
    }
    if !(10_000..=50_000).contains(&args.gas_price_buffer_bps) {
        return Err("arg.gas_price_buffer_bps_out_of_range".to_string());
    }
    Ok(())
}

fn validate_allowed_assets(assets: &[Principal]) -> Result<(), String> {
    if assets.is_empty() {
        return Err("arg.allowed_assets_empty".to_string());
    }
    let native_ledger = current_native_ledger_canister().ok();
    for asset in assets {
        validate_non_anonymous_principal(asset, "arg.allowed_asset_anonymous")?;
        if Some(*asset) == native_ledger {
            return Err("asset.native_ledger_not_wrappable".to_string());
        }
    }
    Ok(())
}

fn validate_query_precompile_allow_args(args: &PrecompileAllowArgs) -> Result<(), String> {
    let target_non_anonymous = args.target != Principal::anonymous();
    let valid = verified_core::kasane_precompiles::icp_query_allowlist_entry_safe_raw(
        args.target.as_slice().len() as u64,
        target_non_anonymous as u64,
        args.method.len() as u64,
        args.method.is_ascii() as u64,
    );
    if !valid && !target_non_anonymous {
        return Err("arg.target_anonymous".to_string());
    }
    if !valid {
        return Err("arg.method_invalid".to_string());
    }
    Ok(())
}

fn validate_update_precompile_allow_args(args: &PrecompileAllowArgs) -> Result<(), String> {
    let target_non_anonymous = args.target != Principal::anonymous();
    let valid = verified_core::kasane_precompiles::icp_query_allowlist_entry_safe_raw(
        args.target.as_slice().len() as u64,
        target_non_anonymous as u64,
        args.method.len() as u64,
        args.method.is_ascii() as u64,
    );
    if !valid && !target_non_anonymous {
        return Err("arg.target_anonymous".to_string());
    }
    if !valid {
        return Err("arg.method_invalid".to_string());
    }
    Ok(())
}

fn precompile_allow_key_for_principal(target: Principal, method: &str) -> Vec<u8> {
    precompile_allow_key(target.as_slice(), method)
}

fn decode_precompile_allow_key_for_principal(key: &[u8]) -> Option<PrecompileAllowedView> {
    let len = usize::from(*key.first()?);
    if len == 0 || len > 29 || key.len() <= 1 + len {
        return None;
    }
    let target = Principal::from_slice(&key[1..1 + len]);
    let method = std::str::from_utf8(&key[1 + len..]).ok()?.to_string();
    Some(PrecompileAllowedView { target, method })
}

fn is_query_precompile_allowed(target: &[u8], method: &str) -> bool {
    if target.is_empty() || target.len() > 29 || method.is_empty() || method.len() > 64 {
        return false;
    }
    let target = Principal::from_slice(target);
    let key = precompile_allow_key_for_principal(target, method);
    with_state(|state| state.query_precompile_allowlist.get(&key).is_some())
}

fn is_update_precompile_allowed(target: &[u8], method: &str) -> bool {
    if target.is_empty()
        || target.len() > 29
        || method.is_empty()
        || method.len() > 64
        || !method.is_ascii()
    {
        return false;
    }
    let target = Principal::from_slice(target);
    let key = precompile_allow_key_for_principal(target, method);
    with_state(|state| state.icp_update_precompile_allowlist.get(&key).is_some())
}

fn validate_evm_address(bytes: &[u8], code: &str) -> Result<(), String> {
    if bytes.len() != 20 {
        return Err(code.to_string());
    }
    Ok(())
}

fn validate_non_anonymous_principal(principal: &Principal, code: &str) -> Result<(), String> {
    if *principal == Principal::anonymous() {
        return Err(code.to_string());
    }
    Ok(())
}

fn runtime_wrap_canister_id(configured: Principal) -> Principal {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = configured;
        ic_cdk::api::canister_self()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        configured
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
            wrap_config: None,
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
    apply_wrap_config_from_init_args(&args);
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
    let bytes = current_runtime_config()
        .wrap_canister_id_bytes()
        .unwrap_or_else(|err| ic_cdk::trap(format!("InvalidRuntimeConfig: {err}")));
    Principal::from_slice(&bytes)
}

fn require_wrap_canister_caller(caller: Principal) -> Result<(), String> {
    if caller == current_wrap_canister_id() {
        Ok(())
    } else {
        Err("auth.wrap_canister_required".to_string())
    }
}

fn apply_wrap_config_from_init_args(args: &InitArgs) {
    let Some(config) = &args.wrap_config else {
        return;
    };
    with_state_mut(|state| {
        state.wrap_fee_policy.set(FeePolicyStored {
            fee_ledger_canister: config.fee_ledger_canister.as_slice().to_vec(),
            cycle_fee_e8s: config.cycle_fee_e8s,
            gas_price_buffer_bps: config.gas_price_buffer_bps,
        });
        state.wrap_evm_config.set(WrapEvmConfigStored {
            wrap_factory_address: args.wrap_factory_address.clone(),
        });
        state
            .wrap_native_ledger_canister
            .set(config.native_ledger_canister.as_slice().to_vec());
        while let Some(entry) = state.wrap_allowed_assets.range(..).next() {
            let key = entry.key().clone();
            state.wrap_allowed_assets.remove(&key);
        }
        for asset in &config.allowed_assets {
            state
                .wrap_allowed_assets
                .insert(asset.as_slice().to_vec(), 1);
        }
    });
}

fn current_native_ledger_canister() -> Result<Principal, String> {
    with_state(|state| principal_from_stored_bytes(state.wrap_native_ledger_canister.get()))
}

fn ensure_asset_allowed(asset: Principal) -> Result<(), String> {
    with_state(|state| {
        if state
            .wrap_allowed_assets
            .get(&asset.as_slice().to_vec())
            .is_some()
        {
            Ok(())
        } else {
            Err("asset.not_allowed".to_string())
        }
    })
}

fn principal_from_stored_bytes(bytes: &[u8]) -> Result<Principal, String> {
    if bytes.is_empty() {
        return Err("wrap_config.unconfigured".to_string());
    }
    if bytes.len() > evm_db::chain_data::wrap_request::PRINCIPAL_MAX_BYTES {
        return Err("arg.principal_invalid".to_string());
    }
    Ok(Principal::from_slice(bytes))
}

fn current_fee_policy() -> Result<FeePolicyView, String> {
    with_state(|state| {
        let stored = state.wrap_fee_policy.get();
        Ok(FeePolicyView {
            fee_ledger_canister: principal_from_stored_bytes(&stored.fee_ledger_canister)?,
            cycle_fee_e8s: stored.cycle_fee_e8s,
            gas_price_buffer_bps: stored.gas_price_buffer_bps,
        })
    })
}

fn wrap_quote_gas_price() -> Result<u128, ApiError> {
    match ic_evm_rpc::rpc_eth_gas_price() {
        Ok(value) => Ok(value),
        Err(err) if err.error_prefix.as_deref() == Some("exec.state.unavailable") => {
            let min_gas_price = with_state(|state| state.chain_state.get().min_gas_price);
            Ok(u128::from(min_gas_price.max(DEFAULT_MIN_FEE_FLOOR)))
        }
        Err(err) => Err(rpc_error_to_api_error(err)),
    }
}

fn current_block_gas_limit() -> u64 {
    with_state(|state| state.chain_state.get().block_gas_limit)
}

fn validate_wrap_gas_limit(gas_limit: u64) -> Result<(), String> {
    if gas_limit == 0 {
        return Err("arg.gas_limit_zero".to_string());
    }
    let configured = current_block_gas_limit();
    let max = if configured == 0 {
        DEFAULT_BLOCK_GAS_LIMIT
    } else {
        configured
    };
    if gas_limit > max {
        return Err("arg.gas_limit_exceeds_block".to_string());
    }
    Ok(())
}

fn clamp_utf8_bytes(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end].to_string()
}

fn clamp_error_code(code: String) -> String {
    clamp_utf8_bytes(&code, MAX_STORED_ERROR_CODE_BYTES)
}

fn validate_stored_ledger_tx_id(tx_id: &[u8]) -> Result<(), String> {
    if tx_id.is_empty() || tx_id.len() > MAX_STORED_LEDGER_TX_ID_BYTES {
        return Err("ledger.tx_id_invalid".to_string());
    }
    Ok(())
}

fn validated_ledger_tx_id(tx_id: Vec<u8>) -> Result<Vec<u8>, String> {
    validate_stored_ledger_tx_id(&tx_id)?;
    Ok(tx_id)
}

fn validate_stored_principal_bytes(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > evm_db::chain_data::wrap_request::PRINCIPAL_MAX_BYTES {
        return Err("arg.principal_invalid".to_string());
    }
    Ok(())
}

fn validate_wrap_request_storage(
    req: &evm_db::chain_data::WrapStoredRequest,
) -> Result<(), String> {
    validate_stored_principal_bytes(&req.caller)?;
    validate_stored_principal_bytes(&req.asset_id)?;
    validate_stored_principal_bytes(&req.fee_ledger_canister)?;
    validate_evm_address(&req.evm_recipient, "arg.evm_recipient_invalid")?;
    if req.amount.len() != 32 {
        return Err("arg.amount_invalid".to_string());
    }
    if let Some(tx_id) = req.result.fee_ledger_tx_id.as_deref() {
        validate_stored_ledger_tx_id(tx_id)?;
    }
    if let Some(tx_id) = req.result.pull_ledger_tx_id.as_deref() {
        validate_stored_ledger_tx_id(tx_id)?;
    }
    if let Some(tx_id) = req.result.withdraw_ledger_tx_id.as_deref() {
        validate_stored_ledger_tx_id(tx_id)?;
    }
    if let Some(tx_id) = req.result.mint_tx_id.as_deref() {
        validate_stored_ledger_tx_id(tx_id)?;
    }
    Ok(())
}

fn sanitize_wrap_request(
    mut req: evm_db::chain_data::WrapStoredRequest,
) -> Result<evm_db::chain_data::WrapStoredRequest, String> {
    if let Some(code) = req.result.error_code.take() {
        req.result.error_code = Some(clamp_error_code(code));
    }
    if let Some(code) = req.result.withdraw_error_code.take() {
        req.result.withdraw_error_code = Some(clamp_error_code(code));
    }
    validate_wrap_request_storage(&req)?;
    Ok(req)
}

fn quote_wrap_request_inner(gas_limit: u64) -> Result<QuoteWrapRequestOk, ApiError> {
    validate_wrap_gas_limit(gas_limit).map_err(|err| api_invalid_argument(&err, &err))?;
    let fee_policy = current_fee_policy().map_err(|err| api_internal(&err, &err))?;
    let base_gas_price = wrap_quote_gas_price()?;
    let charged_gas_price_wei = base_gas_price
        .saturating_mul(u128::from(fee_policy.gas_price_buffer_bps))
        .saturating_add(GAS_PRICE_DENOMINATOR_BPS - 1)
        / GAS_PRICE_DENOMINATOR_BPS;
    let gas_fee_e8s = charged_gas_price_wei
        .saturating_mul(u128::from(gas_limit))
        .saturating_add(WEI_PER_E8S - 1)
        / WEI_PER_E8S;
    let charged_fee_e8s = gas_fee_e8s.saturating_add(u128::from(fee_policy.cycle_fee_e8s));
    Ok(QuoteWrapRequestOk {
        charged_fee_e8s: Nat::from(charged_fee_e8s),
        charged_gas_price_wei: Nat::from(charged_gas_price_wei),
        cycle_fee_e8s: fee_policy.cycle_fee_e8s,
        fee_ledger_canister: fee_policy.fee_ledger_canister,
    })
}

#[allow(dead_code)]
fn derive_wrap_request_id(
    from_owner: &[u8],
    asset_id: &[u8],
    amount: &[u8],
    evm_recipient: &[u8],
    evm_nonce: u64,
    gas_limit: u64,
) -> [u8; 32] {
    let mut keccak = Keccak::v256();
    keccak.update(b"kasane.wrap.request.v1");
    hash_len_prefixed(&mut keccak, from_owner);
    hash_len_prefixed(&mut keccak, asset_id);
    hash_len_prefixed(&mut keccak, amount);
    hash_len_prefixed(&mut keccak, evm_recipient);
    keccak.update(&evm_nonce.to_be_bytes());
    keccak.update(&gas_limit.to_be_bytes());
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    out
}

#[allow(dead_code)]
fn derive_native_deposit_request_id(from_owner: &[u8], deposit_id: &[u8]) -> [u8; 32] {
    let mut keccak = Keccak::v256();
    keccak.update(b"kasane.native.deposit.v2");
    hash_len_prefixed(&mut keccak, from_owner);
    hash_len_prefixed(&mut keccak, deposit_id);
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    out
}

#[allow(dead_code)]
fn hash_len_prefixed(hasher: &mut Keccak, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    hasher.update(&len.to_be_bytes());
    hasher.update(bytes);
}

fn request_memo(request_id: TxId, kind: TransferMemoKind) -> Vec<u8> {
    let kind_byte = match kind {
        TransferMemoKind::Unwrap => 1,
        TransferMemoKind::Fee => 2,
        TransferMemoKind::Pull => 3,
        TransferMemoKind::Withdraw => 4,
    };
    let mut keccak = Keccak::v256();
    keccak.update(b"kasane.wrap.memo.v1");
    keccak.update(&[kind_byte]);
    keccak.update(&request_id.0);
    let mut memo = vec![0u8; 32];
    keccak.finalize(&mut memo);
    memo
}

fn nat_to_be_bytes(value: &Nat) -> Vec<u8> {
    value.0.to_bytes_be()
}

#[ic_cdk::query]
fn quote_wrap_request(args: QuoteWrapRequestArgs) -> Result<QuoteWrapRequestOk, ApiError> {
    validate_non_anonymous_principal(&args.asset_id, "arg.asset_id_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    ensure_asset_allowed(args.asset_id).map_err(|err| api_rejected(&err, &err))?;
    validate_evm_address(&args.evm_recipient, "arg.evm_recipient_invalid")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    if nat_to_u128(&args.amount_e8s)
        .filter(|amount| *amount > 0)
        .is_none()
    {
        return Err(api_invalid_argument(
            "arg.amount_invalid",
            "arg.amount_invalid",
        ));
    }
    quote_wrap_request_inner(args.gas_limit)
}

#[ic_cdk::update]
async fn submit_wrap_request(args: SubmitWrapRequestArgs) -> Result<SubmitWrapRequestOk, ApiError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(api_rejected(&reason, &reason));
    }
    if let Some(reason) = reject_write_reason() {
        return Err(api_rejected(&reason, &reason));
    }
    let caller = msg_caller();
    validate_non_anonymous_principal(&caller, "auth.caller_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    let normalized = normalize_submit_wrap_request(args, caller)?;
    ensure_asset_allowed(Principal::from_slice(&normalized.asset_id))
        .map_err(|err| api_rejected(&err, &err))?;
    let quote = quote_wrap_request_inner(normalized.gas_limit)?;
    validate_wrap_quote_within_approval(&normalized, &quote)
        .map_err(|err| api_rejected(&err, &err))?;
    let charged_fee_e8s = nat_to_u128(&quote.charged_fee_e8s)
        .ok_or_else(|| api_internal("fee.quote_out_of_range", "fee.quote_out_of_range"))?;
    let charged_gas_price_wei = nat_to_u128(&quote.charged_gas_price_wei)
        .ok_or_else(|| api_internal("fee.quote_out_of_range", "fee.quote_out_of_range"))?;
    let request_id = normalized.request_id;
    if let Some(existing) = existing_wrap_request_response(&normalized, caller) {
        return existing;
    }
    reserve_wrap_pending_submission(request_id, caller).map_err(|err| api_rejected(&err, &err))?;
    let req =
        ensure_wrap_request_before_fee(normalized, caller, charged_fee_e8s, charged_gas_price_wei)
            .map_err(|err| {
                clear_wrap_pending_submission(request_id);
                api_rejected(&err, &err)
            })?;
    let fee_ledger_tx_id = attempt_icrc2_transfer_from(
        caller,
        quote.fee_ledger_canister,
        quote.charged_fee_e8s.clone(),
        request_memo(request_id, TransferMemoKind::Fee),
        req.fee_created_at_time,
    )
    .await
    .map_err(|err| {
        record_wrap_request_failure(request_id, map_fee_collection_error(&err), false);
        clear_wrap_pending_submission(request_id);
        let code = map_fee_collection_error(&err);
        api_rejected(&code, &code)
    })?;
    if let Err(err) = record_wrap_fee_collected(request_id, fee_ledger_tx_id.clone()) {
        clear_wrap_pending_submission(request_id);
        return Err(api_rejected(&err, &err));
    }
    enqueue_wrap_request_once(request_id);
    clear_wrap_pending_submission(request_id);
    #[cfg(target_arch = "wasm32")]
    schedule_wrap_worker();
    Ok(SubmitWrapRequestOk {
        request_id: request_id.0.to_vec(),
        charged_fee_e8s: quote.charged_fee_e8s,
        charged_gas_price_wei: quote.charged_gas_price_wei,
        fee_ledger_tx_id,
    })
}

struct NormalizedSubmitWrapRequest {
    request_id: TxId,
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    evm_recipient: Vec<u8>,
    gas_limit: u64,
    max_fee_e8s: u128,
    quoted_gas_price_wei: u128,
    fee_ledger_canister: Principal,
}

fn existing_wrap_request_response(
    args: &NormalizedSubmitWrapRequest,
    caller: Principal,
) -> Option<Result<SubmitWrapRequestOk, ApiError>> {
    with_state(|state| {
        let existing = state.wrap_requests.get(&args.request_id)?;
        if existing.caller.as_slice() != caller.as_slice()
            || existing.asset_id.as_slice() != args.asset_id.as_slice()
            || existing.amount.as_slice() != args.amount.as_slice()
            || existing.evm_recipient.as_slice() != args.evm_recipient.as_slice()
            || existing.gas_limit != args.gas_limit
            || existing.fee_ledger_canister.as_slice() != args.fee_ledger_canister.as_slice()
            || existing.max_fee_e8s != args.max_fee_e8s
            || existing.quoted_gas_price_wei != args.quoted_gas_price_wei
        {
            return Some(Err(api_rejected(
                "request.idempotency_mismatch",
                "request.idempotency_mismatch",
            )));
        }
        let fee_ledger_tx_id = existing.result.fee_ledger_tx_id.clone()?;
        let Some(charged_fee_e8s) = existing.result.charged_fee_e8s else {
            return Some(Err(api_rejected(
                "request.idempotency_incomplete",
                "request.idempotency_incomplete",
            )));
        };
        let Some(charged_gas_price_wei) = existing.result.charged_gas_price_wei else {
            return Some(Err(api_rejected(
                "request.idempotency_incomplete",
                "request.idempotency_incomplete",
            )));
        };
        Some(Ok(SubmitWrapRequestOk {
            request_id: args.request_id.0.to_vec(),
            charged_fee_e8s: Nat::from(charged_fee_e8s),
            charged_gas_price_wei: Nat::from(charged_gas_price_wei),
            fee_ledger_tx_id,
        }))
    })
}

fn reserve_wrap_pending_submission(request_id: TxId, caller: Principal) -> Result<(), String> {
    with_state_mut(|state| {
        if let Some(existing) = state.wrap_pending_submissions.get(&request_id) {
            if existing.is_decode_failure_placeholder() || existing.request_id != request_id.0 {
                state.wrap_pending_submissions.remove(&request_id);
            } else {
                if existing.caller.as_slice() == caller.as_slice() {
                    return Err("request.in_progress".to_string());
                }
                return Err("request.idempotency_mismatch".to_string());
            }
        }
        state.wrap_pending_submissions.insert(
            request_id,
            WrapPendingSubmission {
                caller: caller.as_slice().to_vec(),
                request_id: request_id.0.to_vec(),
            },
        );
        Ok(())
    })
}

fn clear_wrap_pending_submission(request_id: TxId) {
    with_state_mut(|state| {
        state.wrap_pending_submissions.remove(&request_id);
    });
}

fn normalize_submit_wrap_request(
    args: SubmitWrapRequestArgs,
    caller: Principal,
) -> Result<NormalizedSubmitWrapRequest, ApiError> {
    validate_non_anonymous_principal(&args.asset_id, "arg.asset_id_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    validate_evm_address(&args.evm_recipient, "arg.evm_recipient_invalid")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    if amount.iter().all(|&byte| byte == 0) {
        return Err(api_invalid_argument("arg.amount_zero", "arg.amount_zero"));
    }
    let max_fee_e8s = nat_to_u128(&args.max_fee_e8s).ok_or_else(|| {
        api_invalid_argument("arg.max_fee_out_of_range", "arg.max_fee_out_of_range")
    })?;
    let quoted_gas_price_wei = nat_to_u128(&args.quoted_gas_price_wei).ok_or_else(|| {
        api_invalid_argument(
            "arg.quoted_gas_price_out_of_range",
            "arg.quoted_gas_price_out_of_range",
        )
    })?;
    let request_id = TxId(derive_wrap_request_id(
        caller.as_slice(),
        args.asset_id.as_slice(),
        &amount,
        args.evm_recipient.as_slice(),
        args.evm_nonce,
        args.gas_limit,
    ));
    Ok(NormalizedSubmitWrapRequest {
        request_id,
        asset_id: args.asset_id.as_slice().to_vec(),
        amount: amount.to_vec(),
        evm_recipient: args.evm_recipient,
        gas_limit: args.gas_limit,
        max_fee_e8s,
        quoted_gas_price_wei,
        fee_ledger_canister: args.fee_ledger_canister,
    })
}

fn validate_wrap_quote_within_approval(
    args: &NormalizedSubmitWrapRequest,
    quote: &QuoteWrapRequestOk,
) -> Result<(), String> {
    if quote.fee_ledger_canister != args.fee_ledger_canister {
        return Err("fee.ledger_changed".to_string());
    }
    let charged_fee_e8s =
        nat_to_u128(&quote.charged_fee_e8s).ok_or_else(|| "fee.quote_out_of_range".to_string())?;
    let charged_gas_price_wei = nat_to_u128(&quote.charged_gas_price_wei)
        .ok_or_else(|| "fee.quote_out_of_range".to_string())?;
    if charged_fee_e8s > args.max_fee_e8s {
        return Err("fee.quote_exceeded".to_string());
    }
    if charged_gas_price_wei > args.quoted_gas_price_wei {
        return Err("fee.gas_price_exceeded".to_string());
    }
    Ok(())
}

fn ensure_wrap_request_before_fee(
    args: NormalizedSubmitWrapRequest,
    caller: Principal,
    charged_fee_e8s: u128,
    charged_gas_price_wei: u128,
) -> Result<evm_db::chain_data::WrapStoredRequest, String> {
    let request_id = args.request_id;
    with_state_mut(|state| {
        if let Some(existing) = state.wrap_requests.get(&request_id) {
            return Ok(existing);
        }
        let now = current_time_nanos();
        let req = sanitize_wrap_request(evm_db::chain_data::WrapStoredRequest {
            caller: caller.as_slice().to_vec(),
            asset_id: args.asset_id,
            amount: args.amount,
            evm_recipient: args.evm_recipient,
            gas_limit: args.gas_limit,
            fee_ledger_canister: args.fee_ledger_canister.as_slice().to_vec(),
            max_fee_e8s: args.max_fee_e8s,
            quoted_gas_price_wei: args.quoted_gas_price_wei,
            fee_created_at_time: now,
            pull_created_at_time: now,
            withdraw_created_at_time: 0,
            result: evm_db::chain_data::WrapRequestResult {
                status: StoredRequestStatus::Queued,
                pull_ledger_tx_id: None,
                mint_tx_id: None,
                error_code: None,
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                withdraw_in_progress: false,
                mint_failed_recoverable: false,
                fee_ledger_tx_id: None,
                charged_fee_e8s: Some(charged_fee_e8s),
                charged_gas_price_wei: Some(charged_gas_price_wei),
                stage: WrapRequestStage::FeePending,
                updated_at: now,
                mint_nonce: None,
                mint_submitted_at_time: 0,
                mint_submit_status: MintSubmitStatus::NotSubmitted,
            },
        })?;
        state.wrap_requests.insert(request_id, req.clone());
        Ok(req)
    })
}

fn record_wrap_fee_collected(request_id: TxId, fee_ledger_tx_id: Vec<u8>) -> Result<(), String> {
    let fee_ledger_tx_id = validated_ledger_tx_id(fee_ledger_tx_id)?;
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        req.result.fee_ledger_tx_id = Some(fee_ledger_tx_id);
        req.result.error_code = None;
        req.result.stage = WrapRequestStage::FeeCollected;
        req.result.updated_at = current_time_nanos();
        let req = sanitize_wrap_request(req)?;
        state.wrap_requests.insert(request_id, req);
        Ok(())
    })
}

fn enqueue_wrap_request_once(request_id: TxId) {
    with_state_mut(|state| {
        for entry in state.wrap_queue.range(..) {
            if entry.value() == request_id {
                return;
            }
        }
        let mut meta = *state.wrap_queue_meta.get();
        let seq = meta.push();
        state.wrap_queue_meta.set(meta);
        state.wrap_queue.insert(seq, request_id);
    });
}

fn record_wrap_request_failure(request_id: TxId, code: String, mint_failed_recoverable: bool) {
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return;
        };
        req.result.status = StoredRequestStatus::Failed;
        req.result.stage = WrapRequestStage::Failed;
        req.result.error_code = Some(clamp_error_code(code));
        req.result.mint_failed_recoverable = mint_failed_recoverable;
        req.result.updated_at = current_time_nanos();
        if let Ok(req) = sanitize_wrap_request(req) {
            state.wrap_requests.insert(request_id, req);
        }
    });
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn dequeue_wrap_request() -> Option<TxId> {
    let out = with_state_mut(|state| {
        let mut meta = *state.wrap_queue_meta.get();
        let original_head = meta.head;
        let mut found = None;
        while meta.head < meta.tail {
            let seq = meta.head;
            meta.head = meta.head.saturating_add(1);
            if let Some(request_id) = state.wrap_queue.remove(&seq) {
                found = Some(request_id);
                break;
            }
        }
        if meta.head != original_head {
            state.wrap_queue_meta.set(meta);
        }
        found
    });
    out
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn expected_wrap_factory_address() -> Result<Vec<u8>, String> {
    with_state(|state| {
        let stored = state.wrap_evm_config.get();
        validate_evm_address(
            &stored.wrap_factory_address,
            "config.wrap_factory_address_invalid",
        )?;
        Ok(stored.wrap_factory_address.clone())
    })
}

fn encode_factory_mint_for_asset_call_data(
    asset_id: &[u8],
    token_decimals: u8,
    recipient: &[u8],
    amount: &[u8],
) -> Result<Vec<u8>, String> {
    principal_from_stored_bytes(asset_id)?;
    validate_evm_address(recipient, "arg.evm_recipient_invalid")?;
    if amount.len() != 32 {
        return Err("arg.amount_invalid".to_string());
    }
    let mut data = Vec::with_capacity(4 + 32 * 5 + 64);
    data.extend_from_slice(&factory_mint_for_asset_selector());
    data.extend_from_slice(&u256_from_u64(128));
    data.extend_from_slice(&u256_from_u64(u64::from(token_decimals)));
    data.extend_from_slice(&[0u8; 12]);
    data.extend_from_slice(recipient);
    data.extend_from_slice(amount);
    data.extend_from_slice(&u256_from_u64(asset_id.len() as u64));
    data.extend_from_slice(asset_id);
    let padded = (32 - (asset_id.len() % 32)) % 32;
    if padded != 0 {
        data.extend(vec![0u8; padded]);
    }
    Ok(data)
}

fn u256_from_u64(value: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..32].copy_from_slice(&value.to_be_bytes());
    out
}

fn u256_from_u128(value: u128) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[16..32].copy_from_slice(&value.to_be_bytes());
    out
}

fn native_deposit_amount_wei_bytes(amount_e8s: &Nat) -> Result<[u8; 32], ApiError> {
    let amount_e8s = nat_to_u128(amount_e8s).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    let amount_wei = amount_e8s.checked_mul(WEI_PER_E8S).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    Ok(u256_from_u128(amount_wei))
}

fn selector(signature: &[u8]) -> [u8; 4] {
    let mut keccak = Keccak::v256();
    keccak.update(signature);
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    [out[0], out[1], out[2], out[3]]
}

fn factory_mint_for_asset_selector() -> [u8; 4] {
    selector(b"mintForAsset(bytes,uint8,address,uint256)")
}

fn encode_balance_of_call_data(owner: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 32);
    out.extend_from_slice(&selector(b"balanceOf(address)"));
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(owner);
    out
}

fn encode_allowance_call_data(owner: &[u8], spender: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 64);
    out.extend_from_slice(&selector(b"allowance(address,address)"));
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(owner);
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(spender);
    out
}

fn encode_factory_get_token_address_call_data(asset_id: &[u8]) -> Vec<u8> {
    let padded_len = asset_id.len().div_ceil(32) * 32;
    let mut out = Vec::with_capacity(4 + 32 * 3 + padded_len);
    out.extend_from_slice(&selector(b"getTokenAddress(bytes)"));
    out.extend_from_slice(&u256_from_u128(32));
    out.extend_from_slice(&u256_from_u128(asset_id.len() as u128));
    out.extend_from_slice(asset_id);
    out.resize(4 + 32 * 2 + padded_len, 0);
    out
}

fn zero_eth_value_word() -> Vec<u8> {
    vec![0u8; 32]
}

fn decode_u256_be(bytes: &[u8]) -> Result<[u8; 32], ApiError> {
    if bytes.len() < 32 {
        return Err(api_internal(
            "rpc.return_data_short",
            "rpc.return_data_short",
        ));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[..32]);
    Ok(out)
}

fn approval_required_for_readiness(readiness: UnwrapReadiness) -> bool {
    readiness == UnwrapReadiness::InsufficientAllowance
}

fn native_withdraw_receive_amount(amount_e8s: u128, ledger_fee_e8s: u128) -> Result<u128, String> {
    if amount_e8s <= ledger_fee_e8s {
        return Err("native_withdraw.amount_not_above_fee".to_string());
    }
    Ok(amount_e8s - ledger_fee_e8s)
}

#[ic_cdk::query]
fn quote_native_deposit(args: QuoteNativeDepositArgs) -> Result<QuoteNativeDepositOk, ApiError> {
    validate_evm_address(&args.evm_recipient, "arg.evm_recipient_invalid")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    if nat_to_u128(&args.amount_e8s)
        .filter(|amount| *amount > 0)
        .is_none()
    {
        return Err(api_invalid_argument(
            "arg.amount_invalid",
            "arg.amount_invalid",
        ));
    }
    let fee_policy = current_fee_policy().map_err(|err| api_internal(&err, &err))?;
    let native_ledger_canister =
        current_native_ledger_canister().map_err(|err| api_internal(&err, &err))?;
    Ok(QuoteNativeDepositOk {
        charged_fee_e8s: Nat::from(fee_policy.cycle_fee_e8s),
        native_ledger_canister,
        fee_ledger_canister: fee_policy.fee_ledger_canister,
    })
}

#[ic_cdk::query(composite = true)]
async fn quote_native_withdrawal(
    args: QuoteNativeWithdrawalArgs,
) -> Result<QuoteNativeWithdrawalOk, ApiError> {
    validate_non_anonymous_principal(&args.recipient, "arg.recipient_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    let amount = nat_to_u128(&args.amount_e8s).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    if amount == 0 {
        return Err(api_invalid_argument("arg.amount_zero", "arg.amount_zero"));
    }
    let ledger = current_native_ledger_canister().map_err(|err| api_internal(&err, &err))?;
    let fee = fetch_icrc1_fee(ledger)
        .await
        .map_err(|err| api_rejected(&err, &err))?;
    let receive_amount =
        native_withdraw_receive_amount(amount, fee).map_err(|err| api_rejected(&err, &err))?;
    Ok(QuoteNativeWithdrawalOk {
        native_ledger_canister: ledger,
        ledger_fee_e8s: Nat::from(fee),
        receive_amount_e8s: Nat::from(receive_amount),
    })
}

#[ic_cdk::query]
fn get_unwrap_requirements(
    args: GetUnwrapRequirementsArgs,
) -> Result<GetUnwrapRequirementsOk, ApiError> {
    validate_non_anonymous_principal(&args.asset_id, "arg.asset_id_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    validate_evm_address(&args.caller_evm_address, "arg.caller_evm_address_invalid")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    let amount = nat_to_u128(&args.amount_e8s).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    if amount == 0 {
        return Err(api_invalid_argument("arg.amount_zero", "arg.amount_zero"));
    }
    let factory_address =
        expected_wrap_factory_address().map_err(|err| api_internal(&err, &err))?;
    let token_address = fetch_wrapped_token_address(
        args.asset_id.as_slice(),
        args.caller_evm_address.as_slice(),
        factory_address.as_slice(),
    )?;
    let Some(token_address) = token_address else {
        return Ok(GetUnwrapRequirementsOk {
            factory_address,
            wrapped_token_address: None,
            balance: Nat::from(0u8),
            allowance: Nat::from(0u8),
            approve_required: approval_required_for_readiness(UnwrapReadiness::TokenNotDeployed),
            readiness: UnwrapReadiness::TokenNotDeployed,
        });
    };
    let balance = fetch_erc20_balance(&token_address, &args.caller_evm_address)?;
    let allowance =
        fetch_erc20_allowance(&token_address, &args.caller_evm_address, &factory_address)?;
    let balance_u128 = nat_to_u128(&balance)
        .ok_or_else(|| api_internal("erc20.balance_out_of_range", "erc20.balance_out_of_range"))?;
    let allowance_u128 = nat_to_u128(&allowance).ok_or_else(|| {
        api_internal(
            "erc20.allowance_out_of_range",
            "erc20.allowance_out_of_range",
        )
    })?;
    let readiness = if balance_u128 < amount {
        UnwrapReadiness::InsufficientBalance
    } else if allowance_u128 < amount {
        UnwrapReadiness::InsufficientAllowance
    } else {
        UnwrapReadiness::Ready
    };
    Ok(GetUnwrapRequirementsOk {
        factory_address,
        wrapped_token_address: Some(token_address),
        balance,
        allowance,
        approve_required: approval_required_for_readiness(readiness),
        readiness,
    })
}

#[ic_cdk::update]
async fn submit_native_deposit(
    args: SubmitNativeDepositArgs,
) -> Result<SubmitNativeDepositOk, ApiError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(api_rejected(&reason, &reason));
    }
    if let Some(reason) = reject_write_reason() {
        return Err(api_rejected(&reason, &reason));
    }
    let caller = msg_caller();
    validate_non_anonymous_principal(&caller, "auth.caller_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    if args.deposit_id.len() != 32 {
        return Err(api_invalid_argument(
            "arg.deposit_id_invalid",
            "arg.deposit_id_invalid",
        ));
    }
    validate_evm_address(&args.evm_recipient, "arg.evm_recipient_invalid")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    if amount.iter().all(|&byte| byte == 0) {
        return Err(api_invalid_argument("arg.amount_zero", "arg.amount_zero"));
    }
    let amount_wei = native_deposit_amount_wei_bytes(&args.amount_e8s)?;
    let max_fee_e8s = nat_to_u128(&args.max_fee_e8s).ok_or_else(|| {
        api_invalid_argument("arg.max_fee_out_of_range", "arg.max_fee_out_of_range")
    })?;
    let (fee_policy, native_ledger) =
        prepare_native_deposit_funding(args.fee_ledger_canister, max_fee_e8s)?;

    let request_id = TxId(derive_native_deposit_request_id(
        caller.as_slice(),
        args.deposit_id.as_slice(),
    ));
    if let Some(existing) =
        existing_native_deposit_response(request_id, amount, args.evm_recipient.as_slice(), caller)
    {
        return existing;
    }
    reserve_wrap_pending_submission(request_id, caller).map_err(|err| api_rejected(&err, &err))?;
    let mut req = ensure_native_deposit_request_before_fee(NativeDepositRequestDraft {
        request_id,
        caller,
        native_ledger,
        amount: amount.to_vec(),
        evm_recipient: args.evm_recipient.clone(),
        fee_ledger_canister: fee_policy.fee_ledger_canister,
        max_fee_e8s,
        charged_fee_e8s: fee_policy.cycle_fee_e8s,
    })
    .map_err(|err| {
        clear_wrap_pending_submission(request_id);
        api_rejected(&err, &err)
    })?;

    if req.result.fee_ledger_tx_id.is_none() {
        let fee_ledger_tx_id = attempt_icrc2_transfer_from(
            caller,
            fee_policy.fee_ledger_canister,
            Nat::from(fee_policy.cycle_fee_e8s),
            request_memo(request_id, TransferMemoKind::Fee),
            req.fee_created_at_time,
        )
        .await
        .map_err(|err| {
            record_wrap_request_failure(request_id, map_fee_collection_error(&err), false);
            clear_wrap_pending_submission(request_id);
            let code = map_fee_collection_error(&err);
            api_rejected(&code, &code)
        })?;
        record_native_deposit_fee_collected(request_id, fee_ledger_tx_id).map_err(|err| {
            clear_wrap_pending_submission(request_id);
            api_rejected(&err, &err)
        })?;
        req = with_state(|state| state.wrap_requests.get(&request_id))
            .ok_or_else(|| api_internal("request.not_found", "request.not_found"))?;
    }

    if req.result.pull_ledger_tx_id.is_none() {
        let pull = attempt_icrc2_transfer_from(
            caller,
            native_ledger,
            args.amount_e8s.clone(),
            request_memo(request_id, TransferMemoKind::Pull),
            req.pull_created_at_time,
        )
        .await;
        match pull {
            Ok(pull_ledger_tx_id) => {
                record_native_deposit_pulled(request_id, pull_ledger_tx_id).map_err(|err| {
                    clear_wrap_pending_submission(request_id);
                    api_rejected(&err, &err)
                })?;
            }
            Err(err) => {
                record_wrap_request_failure(request_id, err.clone(), false);
                clear_wrap_pending_submission(request_id);
                return Err(api_rejected(&err, &err));
            }
        }
        req = with_state(|state| state.wrap_requests.get(&request_id))
            .ok_or_else(|| api_internal("request.not_found", "request.not_found"))?;
    }

    if req.result.mint_tx_id.is_none() || req.result.status != StoredRequestStatus::Succeeded {
        finalize_native_deposit_credit(request_id, &args.evm_recipient, amount_wei)?;
    }
    clear_wrap_pending_submission(request_id);
    let fee_ledger_tx_id = with_state(|state| {
        state
            .wrap_requests
            .get(&request_id)
            .and_then(|req| req.result.fee_ledger_tx_id)
    })
    .ok_or_else(|| api_internal("request.missing_fee_tx", "request.missing_fee_tx"))?;
    Ok(SubmitNativeDepositOk {
        request_id: request_id.0.to_vec(),
        charged_fee_e8s: Nat::from(fee_policy.cycle_fee_e8s),
        fee_ledger_tx_id,
    })
}

fn existing_native_deposit_response(
    request_id: TxId,
    amount: [u8; 32],
    evm_recipient: &[u8],
    caller: Principal,
) -> Option<Result<SubmitNativeDepositOk, ApiError>> {
    with_state(|state| {
        let existing = state.wrap_requests.get(&request_id)?;
        if existing.caller.as_slice() != caller.as_slice()
            || existing.amount != amount
            || existing.evm_recipient != evm_recipient
        {
            return Some(Err(api_rejected(
                "request.idempotency_mismatch",
                "request.idempotency_mismatch",
            )));
        }
        if existing.result.status != StoredRequestStatus::Succeeded {
            return None;
        }
        let fee_ledger_tx_id = existing.result.fee_ledger_tx_id.clone()?;
        let charged_fee_e8s = existing.result.charged_fee_e8s?;
        Some(Ok(SubmitNativeDepositOk {
            request_id: request_id.0.to_vec(),
            charged_fee_e8s: Nat::from(charged_fee_e8s),
            fee_ledger_tx_id,
        }))
    })
}

fn prepare_native_deposit_funding(
    fee_ledger_canister: Principal,
    max_fee_e8s: u128,
) -> Result<(FeePolicyView, Principal), ApiError> {
    let fee_policy = current_fee_policy().map_err(|err| api_internal(&err, &err))?;
    let native_ledger = current_native_ledger_canister().map_err(|err| api_internal(&err, &err))?;
    if fee_policy.fee_ledger_canister != fee_ledger_canister {
        return Err(api_rejected("fee.ledger_changed", "fee.ledger_changed"));
    }
    if u128::from(fee_policy.cycle_fee_e8s) > max_fee_e8s {
        return Err(api_rejected("fee.quote_exceeded", "fee.quote_exceeded"));
    }
    Ok((fee_policy, native_ledger))
}

struct NativeDepositRequestDraft {
    request_id: TxId,
    caller: Principal,
    native_ledger: Principal,
    amount: Vec<u8>,
    evm_recipient: Vec<u8>,
    fee_ledger_canister: Principal,
    max_fee_e8s: u128,
    charged_fee_e8s: u64,
}

fn ensure_native_deposit_request_before_fee(
    draft: NativeDepositRequestDraft,
) -> Result<evm_db::chain_data::WrapStoredRequest, String> {
    with_state_mut(|state| {
        if let Some(existing) = state.wrap_requests.get(&draft.request_id) {
            return Ok(existing);
        }
        let now = current_time_nanos();
        let req = sanitize_wrap_request(evm_db::chain_data::WrapStoredRequest {
            caller: draft.caller.as_slice().to_vec(),
            asset_id: draft.native_ledger.as_slice().to_vec(),
            amount: draft.amount,
            evm_recipient: draft.evm_recipient,
            gas_limit: 0,
            fee_ledger_canister: draft.fee_ledger_canister.as_slice().to_vec(),
            max_fee_e8s: draft.max_fee_e8s,
            quoted_gas_price_wei: 0,
            fee_created_at_time: now,
            pull_created_at_time: now,
            withdraw_created_at_time: 0,
            result: evm_db::chain_data::WrapRequestResult {
                status: StoredRequestStatus::Running,
                pull_ledger_tx_id: None,
                mint_tx_id: None,
                error_code: None,
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                withdraw_in_progress: false,
                mint_failed_recoverable: false,
                fee_ledger_tx_id: None,
                charged_fee_e8s: Some(u128::from(draft.charged_fee_e8s)),
                charged_gas_price_wei: Some(0),
                stage: WrapRequestStage::FeePending,
                updated_at: now,
                mint_nonce: None,
                mint_submitted_at_time: 0,
                mint_submit_status: MintSubmitStatus::NotSubmitted,
            },
        })?;
        state.wrap_requests.insert(draft.request_id, req.clone());
        Ok(req)
    })
}

fn record_native_deposit_fee_collected(
    request_id: TxId,
    fee_ledger_tx_id: Vec<u8>,
) -> Result<(), String> {
    let fee_ledger_tx_id = validated_ledger_tx_id(fee_ledger_tx_id)?;
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        req.result.fee_ledger_tx_id = Some(fee_ledger_tx_id);
        req.result.stage = WrapRequestStage::FeeCollected;
        req.result.updated_at = current_time_nanos();
        req.result.error_code = None;
        let req = sanitize_wrap_request(req)?;
        state.wrap_requests.insert(request_id, req);
        Ok(())
    })
}

fn record_native_deposit_pulled(
    request_id: TxId,
    pull_ledger_tx_id: Vec<u8>,
) -> Result<(), String> {
    let pull_ledger_tx_id = validated_ledger_tx_id(pull_ledger_tx_id)?;
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        req.result.pull_ledger_tx_id = Some(pull_ledger_tx_id);
        req.result.stage = WrapRequestStage::Pulled;
        req.result.updated_at = current_time_nanos();
        req.result.error_code = None;
        let req = sanitize_wrap_request(req)?;
        state.wrap_requests.insert(request_id, req);
        Ok(())
    })
}

fn finalize_native_deposit_credit(
    request_id: TxId,
    evm_recipient: &[u8],
    amount_wei: [u8; 32],
) -> Result<(), ApiError> {
    let outcome = {
        let mut recipient = [0u8; 20];
        recipient.copy_from_slice(evm_recipient);
        credit_native_deposit_internal(request_id.0, recipient, amount_wei)
    };
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return;
        };
        req.result.updated_at = current_time_nanos();
        match outcome {
            Ok(()) => {
                req.result.status = StoredRequestStatus::Succeeded;
                req.result.stage = WrapRequestStage::Succeeded;
                req.result.mint_tx_id = Some(request_id.0.to_vec());
                req.result.error_code = None;
                req.result.mint_failed_recoverable = false;
            }
            Err(err) => {
                req.result.status = StoredRequestStatus::Failed;
                req.result.stage = WrapRequestStage::Failed;
                req.result.error_code = Some(clamp_error_code(format!(
                    "evm_gateway.credit_failed:{}",
                    api_error_code(err)
                )));
                req.result.mint_failed_recoverable = true;
            }
        }
        if let Ok(req) = sanitize_wrap_request(req) {
            state.wrap_requests.insert(request_id, req);
        }
    });
    Ok(())
}

#[ic_cdk::update]
fn dispatch_unwrap_request(
    args: DispatchUnwrapRequestArgs,
) -> Result<DispatchUnwrapRequestOk, ApiError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(api_rejected(&reason, &reason));
    }
    reject_data_plane_write()?;
    require_wrap_canister_caller(msg_caller()).map_err(|err| api_rejected(&err, &err))?;
    let request_id = tx_id_from_bytes(args.request_id.clone())
        .ok_or_else(|| api_invalid_argument("arg.request_id_invalid", "arg.request_id_invalid"))?;
    validate_non_anonymous_principal(&args.asset_id, "arg.asset_id_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    validate_non_anonymous_principal(&args.recipient, "arg.recipient_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    if amount.iter().all(|&byte| byte == 0) {
        return Err(api_invalid_argument("arg.amount_zero", "arg.amount_zero"));
    }
    insert_unwrap_dispatch_request(
        request_id,
        args.asset_id.as_slice().to_vec(),
        amount,
        args.recipient.as_slice().to_vec(),
    )
    .map_err(|err| api_rejected(&err, &err))?;
    Ok(DispatchUnwrapRequestOk {
        request_id: request_id.0.to_vec(),
    })
}

#[ic_cdk::update]
async fn dispatch_native_withdrawal_request(
    args: DispatchNativeWithdrawalRequestArgs,
) -> Result<DispatchUnwrapRequestOk, ApiError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(api_rejected(&reason, &reason));
    }
    reject_data_plane_write()?;
    require_wrap_canister_caller(msg_caller()).map_err(|err| api_rejected(&err, &err))?;
    let request_id = tx_id_from_bytes(args.request_id.clone())
        .ok_or_else(|| api_invalid_argument("arg.request_id_invalid", "arg.request_id_invalid"))?;
    validate_non_anonymous_principal(&args.recipient, "arg.recipient_anonymous")
        .map_err(|err| api_invalid_argument(&err, &err))?;
    let amount_e8s = nat_to_u128(&args.amount_e8s).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    if amount_e8s == 0 {
        return Err(api_invalid_argument("arg.amount_zero", "arg.amount_zero"));
    }
    let ledger = current_native_ledger_canister().map_err(|err| api_internal(&err, &err))?;
    let fee = fetch_icrc1_fee(ledger)
        .await
        .map_err(|err| api_rejected(&err, &err))?;
    native_withdraw_receive_amount(amount_e8s, fee).map_err(|err| api_rejected(&err, &err))?;
    let amount = u256_from_u128(amount_e8s);
    insert_unwrap_dispatch_request(
        request_id,
        NATIVE_WITHDRAW_ASSET_MARKER.to_vec(),
        amount,
        args.recipient.as_slice().to_vec(),
    )
    .map_err(|err| api_rejected(&err, &err))?;
    Ok(DispatchUnwrapRequestOk {
        request_id: request_id.0.to_vec(),
    })
}

fn insert_unwrap_dispatch_request(
    request_id: TxId,
    asset_id: Vec<u8>,
    amount: [u8; 32],
    recipient: Vec<u8>,
) -> Result<(), String> {
    let out = with_state_mut(|state| {
        if let Some(existing) = state.unwrap_requests.get(&request_id) {
            if existing.asset_id != asset_id
                || existing.amount != amount
                || existing.recipient != recipient
            {
                return Err("request.idempotency_mismatch".to_string());
            }
            return Ok(());
        }
        state.unwrap_requests.insert(
            request_id,
            UnwrapDispatchRequest {
                asset_id,
                amount,
                recipient,
                status: UnwrapRequestStatus::Queued,
                ledger_tx_id: None,
                error_code: None,
                updated_at: current_time_nanos(),
                transfer_created_at_time: 0,
            },
        );
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(seq, request_id);
        Ok(())
    });
    out?;
    #[cfg(target_arch = "wasm32")]
    schedule_unwrap_dispatch();
    Ok(())
}

async fn attempt_icrc2_transfer_from(
    caller: Principal,
    ledger: Principal,
    amount: Nat,
    memo: Vec<u8>,
    created_at_time: u64,
) -> Result<Vec<u8>, String> {
    let arg = Icrc2TransferFromArg {
        from: Icrc1Account {
            owner: caller,
            subaccount: None,
        },
        spender_subaccount: None,
        to: Icrc1Account {
            owner: ic_cdk::api::canister_self(),
            subaccount: None,
        },
        amount,
        fee: None,
        memo: Some(memo),
        created_at_time: Some(created_at_time),
    };
    let call_result = ic_cdk::call::Call::unbounded_wait(ledger, "icrc2_transfer_from")
        .with_arg(arg)
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<Nat, Icrc2TransferFromError>,)>() {
            Ok((Ok(block_index),)) => Ok(nat_to_be_bytes(&block_index)),
            Ok((Err(Icrc2TransferFromError::Duplicate { duplicate_of }),)) => {
                Ok(nat_to_be_bytes(&duplicate_of))
            }
            Ok((Err(err),)) => Err(format!(
                "ledger.transfer_from_failed:{}",
                transfer_from_error_to_code(&err)
            )),
            Err(err) => Err(format!("ledger.decode_failed:{err}")),
        },
        Err(err) => Err(format!("ledger.call_failed:{err}")),
    }
}

async fn attempt_icrc1_transfer(
    ledger: Principal,
    recipient: Principal,
    amount: Nat,
    memo: Vec<u8>,
    created_at_time: u64,
) -> Result<Vec<u8>, String> {
    let arg = Icrc1TransferArg {
        from_subaccount: None,
        to: Icrc1Account {
            owner: recipient,
            subaccount: None,
        },
        amount,
        fee: None,
        memo: Some(memo),
        created_at_time: Some(created_at_time),
    };
    let call_result = ic_cdk::call::Call::unbounded_wait(ledger, "icrc1_transfer")
        .with_arg(arg)
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<Nat, Icrc1TransferError>,)>() {
            Ok((Ok(block_index),)) => Ok(nat_to_be_bytes(&block_index)),
            Ok((Err(Icrc1TransferError::Duplicate { duplicate_of }),)) => {
                Ok(nat_to_be_bytes(&duplicate_of))
            }
            Ok((Err(err),)) => Err(format!(
                "ledger.transfer_failed:{}",
                transfer_error_to_code(&err)
            )),
            Err(err) => Err(format!("ledger.decode_failed:{err}")),
        },
        Err(err) => Err(format!("ledger.call_failed:{err}")),
    }
}

async fn fetch_icrc1_fee(ledger: Principal) -> Result<u128, String> {
    let call_result = ic_cdk::call::Call::unbounded_wait(ledger, "icrc1_fee").await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Nat,)>() {
            Ok((fee,)) => nat_to_u128(&fee).ok_or_else(|| "ledger.fee_out_of_range".to_string()),
            Err(err) => Err(format!("ledger.fee_decode_failed:{err}")),
        },
        Err(err) => Err(format!("ledger.fee_call_failed:{err}")),
    }
}

fn fetch_wrapped_token_address(
    asset_id: &[u8],
    caller_evm_address: &[u8],
    factory: &[u8],
) -> Result<Option<Vec<u8>>, ApiError> {
    let result = rpc_eth_call_object(RpcCallObjectView {
        to: Some(factory.to_vec()),
        from: Some(caller_evm_address.to_vec()),
        gas: Some(500_000),
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(zero_eth_value_word()),
        data: Some(encode_factory_get_token_address_call_data(asset_id)),
    })
    .map_err(rpc_error_to_api_error)?;
    if result.return_data.len() < 32 {
        return Ok(None);
    }
    let address = result.return_data[result.return_data.len() - 20..].to_vec();
    if address.iter().all(|byte| *byte == 0) {
        return Ok(None);
    }
    Ok(Some(address))
}

fn fetch_erc20_balance(token: &[u8], owner: &[u8]) -> Result<Nat, ApiError> {
    let result = rpc_eth_call_object(RpcCallObjectView {
        to: Some(token.to_vec()),
        from: Some(owner.to_vec()),
        gas: Some(500_000),
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(zero_eth_value_word()),
        data: Some(encode_balance_of_call_data(owner)),
    })
    .map_err(rpc_error_to_api_error)?;
    Ok(Nat(BigUint::from_bytes_be(&decode_u256_be(
        result.return_data.as_slice(),
    )?)))
}

fn fetch_erc20_allowance(token: &[u8], owner: &[u8], spender: &[u8]) -> Result<Nat, ApiError> {
    let result = rpc_eth_call_object(RpcCallObjectView {
        to: Some(token.to_vec()),
        from: Some(owner.to_vec()),
        gas: Some(500_000),
        gas_price: None,
        nonce: None,
        max_fee_per_gas: None,
        max_priority_fee_per_gas: None,
        chain_id: None,
        tx_type: None,
        access_list: None,
        value: Some(zero_eth_value_word()),
        data: Some(encode_allowance_call_data(owner, spender)),
    })
    .map_err(rpc_error_to_api_error)?;
    Ok(Nat(BigUint::from_bytes_be(&decode_u256_be(
        result.return_data.as_slice(),
    )?)))
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
async fn fetch_asset_decimals(asset_id: &[u8]) -> Result<u8, String> {
    let ledger = principal_from_stored_bytes(asset_id)?;
    let call_result = ic_cdk::call::Call::unbounded_wait(ledger, "icrc1_metadata").await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Vec<(String, Icrc1MetadataValue)>,)>() {
            Ok((metadata,)) => decode_asset_decimals(metadata.as_slice()),
            Err(err) => Err(format!("wrap.asset_metadata_failed:decode_failed:{err}")),
        },
        Err(err) => Err(format!("wrap.asset_metadata_failed:call_failed:{err}")),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn decode_asset_decimals(metadata: &[(String, Icrc1MetadataValue)]) -> Result<u8, String> {
    for (key, value) in metadata {
        if key != "icrc1:decimals" {
            continue;
        }
        let Icrc1MetadataValue::Nat(value) = value else {
            return Err("wrap.asset_decimals_invalid".to_string());
        };
        let decimals =
            nat_to_u128(value).ok_or_else(|| "wrap.asset_decimals_invalid".to_string())?;
        return u8::try_from(decimals).map_err(|_| "wrap.asset_decimals_invalid".to_string());
    }
    Err("wrap.asset_metadata_failed:decimals_missing".to_string())
}

fn map_fee_collection_error(err: &str) -> String {
    format!("fee.{err}")
}

fn transfer_error_to_code(error: &Icrc1TransferError) -> String {
    match error {
        Icrc1TransferError::BadFee { expected_fee } => format!("bad_fee:{}", expected_fee.0),
        Icrc1TransferError::BadBurn { min_burn_amount } => {
            format!("bad_burn:{}", min_burn_amount.0)
        }
        Icrc1TransferError::InsufficientFunds { balance } => {
            format!("insufficient_funds:{}", balance.0)
        }
        Icrc1TransferError::TooOld => "too_old".to_string(),
        Icrc1TransferError::CreatedInFuture { ledger_time } => {
            format!("created_in_future:{ledger_time}")
        }
        Icrc1TransferError::TemporarilyUnavailable => "temporarily_unavailable".to_string(),
        Icrc1TransferError::Duplicate { duplicate_of } => {
            format!("duplicate:{}", duplicate_of.0)
        }
        Icrc1TransferError::GenericError {
            error_code,
            message,
        } => format!("generic_error:{}:{message}", error_code.0),
    }
}

fn transfer_from_error_to_code(error: &Icrc2TransferFromError) -> String {
    match error {
        Icrc2TransferFromError::BadFee { expected_fee } => format!("bad_fee:{}", expected_fee.0),
        Icrc2TransferFromError::BadBurn { min_burn_amount } => {
            format!("bad_burn:{}", min_burn_amount.0)
        }
        Icrc2TransferFromError::InsufficientFunds { balance } => {
            format!("insufficient_funds:{}", balance.0)
        }
        Icrc2TransferFromError::InsufficientAllowance { allowance } => {
            format!("insufficient_allowance:{}", allowance.0)
        }
        Icrc2TransferFromError::TooOld => "too_old".to_string(),
        Icrc2TransferFromError::CreatedInFuture { ledger_time } => {
            format!("created_in_future:{ledger_time}")
        }
        Icrc2TransferFromError::TemporarilyUnavailable => "temporarily_unavailable".to_string(),
        Icrc2TransferFromError::Duplicate { duplicate_of } => {
            format!("duplicate:{}", duplicate_of.0)
        }
        Icrc2TransferFromError::GenericError {
            error_code,
            message,
        } => format!("generic_error:{}:{message}", error_code.0),
    }
}

#[ic_cdk::query]
fn get_fee_policy() -> Result<FeePolicyView, String> {
    current_fee_policy()
}

#[ic_cdk::query]
fn get_allowed_assets() -> Result<Vec<Principal>, String> {
    with_state(|state| {
        let mut out = Vec::new();
        for entry in state.wrap_allowed_assets.iter() {
            out.push(principal_from_stored_bytes(entry.key())?);
        }
        Ok(out)
    })
}

#[ic_cdk::query]
fn get_query_precompile_allowlist() -> Vec<PrecompileAllowedView> {
    with_state(|state| {
        state
            .query_precompile_allowlist
            .iter()
            .filter_map(|entry| decode_precompile_allow_key_for_principal(entry.key()))
            .collect()
    })
}

#[ic_cdk::query]
fn get_update_precompile_allowlist() -> Vec<PrecompileAllowedView> {
    with_state(|state| {
        state
            .icp_update_precompile_allowlist
            .iter()
            .filter_map(|entry| decode_precompile_allow_key_for_principal(entry.key()))
            .collect()
    })
}

#[ic_cdk::query]
fn get_wrap_runtime_config() -> Result<WrapRuntimeConfigView, String> {
    let fee_policy = current_fee_policy()?;
    let native_ledger_canister = current_native_ledger_canister()?;
    let wrap_factory_address = expected_wrap_factory_address()?;
    let allowed_assets = get_allowed_assets()?;
    Ok(WrapRuntimeConfigView {
        native_ledger_canister,
        fee_ledger_canister: fee_policy.fee_ledger_canister,
        allowed_assets,
        wrap_factory_address,
    })
}

#[ic_cdk::query]
fn get_request(request_id: Vec<u8>) -> Option<RequestOverview> {
    let request_id = tx_id_from_bytes(request_id)?;
    with_state(|state| {
        if let Some(req) = state.wrap_requests.get(&request_id) {
            let result = &req.result;
            return Some(RequestOverview {
                kind: if req.gas_limit == 0 {
                    RequestKind::NativeDeposit
                } else {
                    RequestKind::Wrap
                },
                request_id: request_id.0.to_vec(),
                status: request_status_to_view(result.status),
                stage: Some(wrap_request_stage_to_view(result.stage)),
                error: result.error_code.as_deref().map(request_error_view),
                fee_ledger_tx_id: result.fee_ledger_tx_id.clone(),
                pull_ledger_tx_id: result.pull_ledger_tx_id.clone(),
                mint_tx_id: result.mint_tx_id.clone(),
                withdraw_ledger_tx_id: result.withdraw_ledger_tx_id.clone(),
                recoverable: result.mint_failed_recoverable,
                withdrawn: result.withdrawn,
                withdraw_in_progress: result.withdraw_in_progress,
                withdraw_error: result
                    .withdraw_error_code
                    .as_deref()
                    .map(request_error_view),
                ledger_tx_id: None,
                dispatch_status: None,
                dispatch_error: None,
                charged_fee_e8s: result.charged_fee_e8s.map(Nat::from),
                charged_gas_price_wei: result.charged_gas_price_wei.map(Nat::from),
            });
        }
        state.unwrap_requests.get(&request_id).map(|req| {
            let (status, dispatch_status) = unwrap_request_status_to_request_status(req.status);
            RequestOverview {
                kind: if is_native_withdraw_dispatch_request(&req) {
                    RequestKind::NativeWithdrawal
                } else {
                    RequestKind::Unwrap
                },
                request_id: request_id.0.to_vec(),
                status,
                stage: Some(unwrap_request_stage_to_view(req.status)),
                error: req.error_code.as_deref().map(request_error_view),
                fee_ledger_tx_id: None,
                pull_ledger_tx_id: None,
                mint_tx_id: None,
                withdraw_ledger_tx_id: None,
                recoverable: req.status == UnwrapRequestStatus::DispatchFailed,
                withdrawn: false,
                withdraw_in_progress: req.status == UnwrapRequestStatus::Dispatching,
                withdraw_error: req.error_code.as_deref().map(request_error_view),
                ledger_tx_id: req.ledger_tx_id.clone(),
                dispatch_status: Some(dispatch_status),
                dispatch_error: req.error_code.clone(),
                charged_fee_e8s: None,
                charged_gas_price_wei: None,
            }
        })
    })
}

#[ic_cdk::query]
fn get_native_deposit_result(request_id: Vec<u8>) -> Option<RequestOverview> {
    get_request(request_id)
}

#[ic_cdk::query]
fn get_icp_update_request(request_id: Vec<u8>) -> Option<IcpUpdateRequestView> {
    let request_id = tx_id_from_bytes(request_id)?;
    with_state(|state| {
        state.icp_update_requests.get(&request_id).map(|req| {
            let target = Principal::from_slice(&req.target);
            IcpUpdateRequestView {
                request_id: request_id.0.to_vec(),
                tx_id: req.tx_id.0.to_vec(),
                block_number: req.block_number,
                tx_index: req.tx_index,
                log_index: req.log_index,
                tx_kind: tx_kind_to_icp_update_view(req.tx_kind),
                evm_sender: req.evm_sender.to_vec(),
                ic_caller: req
                    .ic_caller
                    .as_ref()
                    .map(|bytes| Principal::from_slice(bytes)),
                target,
                method: req.method,
                status: icp_update_request_status_to_view(req.status),
                reply: req.reply,
                error: req.error_code,
                updated_at: req.updated_at,
            }
        })
    })
}

#[ic_cdk::update]
fn retry_request(args: RetryRequestArgs) -> Result<RequestOverview, ApiError> {
    reject_data_plane_write()?;
    retry_unwrap_dispatch(args.request_id)
}

#[ic_cdk::update]
fn retry_native_withdrawal(args: RetryRequestArgs) -> Result<RequestOverview, ApiError> {
    reject_data_plane_write()?;
    retry_unwrap_dispatch(args.request_id)
}

#[ic_cdk::update]
fn retry_native_deposit(args: RetryRequestArgs) -> Result<RequestOverview, ApiError> {
    reject_data_plane_write()?;
    let request_id = tx_id_from_bytes(args.request_id)
        .ok_or_else(|| api_invalid_argument("arg.request_id_invalid", "arg.request_id_invalid"))?;
    let req_result = with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        if req.gas_limit != 0 || !req.result.mint_failed_recoverable {
            return Err("native_deposit.retry_invalid_state".to_string());
        }
        if req.result.status == StoredRequestStatus::Running {
            return Err("native_deposit.retry_in_progress".to_string());
        }
        if req.result.pull_ledger_tx_id.is_none() {
            return Err("native_deposit.retry_missing_pull".to_string());
        }
        req.result.status = StoredRequestStatus::Running;
        state.wrap_requests.insert(request_id, req.clone());
        Ok(req)
    });
    let req = req_result.map_err(|err| api_rejected(&err, &err))?;

    let amount_e8s = Nat(BigUint::from_bytes_be(&req.amount));
    let outcome = native_deposit_amount_wei_bytes(&amount_e8s).and_then(|amount_wei| {
        let mut recipient = [0u8; 20];
        recipient.copy_from_slice(&req.evm_recipient);
        credit_native_deposit_internal(request_id.0, recipient, amount_wei)
    });
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return;
        };
        match outcome {
            Ok(()) => {
                req.result.status = StoredRequestStatus::Succeeded;
                req.result.mint_tx_id = Some(request_id.0.to_vec());
                req.result.error_code = None;
                req.result.mint_failed_recoverable = false;
            }
            Err(err) => {
                req.result.status = StoredRequestStatus::Failed;
                req.result.error_code =
                    Some(format!("evm_gateway.credit_failed:{}", api_error_code(err)));
                req.result.mint_failed_recoverable = true;
            }
        }
        state.wrap_requests.insert(request_id, req);
    });
    get_request(request_id.0.to_vec())
        .ok_or_else(|| api_internal("request.not_found", "request.not_found"))
}

#[ic_cdk::update]
async fn recover_failed_wrap(args: RecoverFailedWrapArgs) -> Result<RequestOverview, ApiError> {
    reject_data_plane_write()?;
    let request_id = tx_id_from_bytes(args.request_id)
        .ok_or_else(|| api_invalid_argument("arg.request_id_invalid", "arg.request_id_invalid"))?;
    let req_result = with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        if req.gas_limit == 0
            || req.result.status != StoredRequestStatus::Failed
            || !req.result.mint_failed_recoverable
            || req.result.pull_ledger_tx_id.is_none()
        {
            return Err("wrap.recover_invalid_state".to_string());
        }
        if req.result.withdraw_in_progress {
            return Err("wrap.recover_in_progress".to_string());
        }
        if req.result.withdrawn || req.result.withdraw_ledger_tx_id.is_some() {
            return Err("wrap.recover_already_withdrawn".to_string());
        }
        if req.withdraw_created_at_time == 0 {
            req.withdraw_created_at_time = current_time_nanos();
        }
        req.result.withdraw_in_progress = true;
        req.result.stage = WrapRequestStage::Refunding;
        req.result.updated_at = current_time_nanos();
        state.wrap_requests.insert(request_id, req.clone());
        Ok(req)
    });
    let req = req_result.map_err(|err| api_rejected(&err, &err))?;

    let caller =
        principal_from_stored_bytes(&req.caller).map_err(|err| api_internal(&err, &err))?;
    let asset =
        principal_from_stored_bytes(&req.asset_id).map_err(|err| api_internal(&err, &err))?;
    let transfer = attempt_icrc1_transfer(
        asset,
        caller,
        Nat(BigUint::from_bytes_be(&req.amount)),
        request_memo(request_id, TransferMemoKind::Withdraw),
        req.withdraw_created_at_time,
    )
    .await;

    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return;
        };
        req.result.withdraw_in_progress = false;
        req.result.updated_at = current_time_nanos();
        match transfer {
            Ok(ledger_tx_id) => {
                req.result.withdrawn = true;
                req.result.stage = WrapRequestStage::Refunded;
                match validated_ledger_tx_id(ledger_tx_id) {
                    Ok(tx_id) => {
                        req.result.withdraw_ledger_tx_id = Some(tx_id);
                        req.result.withdraw_error_code = None;
                    }
                    Err(err) => {
                        req.result.withdrawn = false;
                        req.result.stage = WrapRequestStage::Failed;
                        req.result.withdraw_error_code = Some(err);
                    }
                }
            }
            Err(err) => {
                req.result.stage = WrapRequestStage::Failed;
                req.result.withdraw_error_code = Some(clamp_error_code(err));
            }
        }
        if let Ok(req) = sanitize_wrap_request(req) {
            state.wrap_requests.insert(request_id, req);
        }
    });
    get_request(request_id.0.to_vec())
        .ok_or_else(|| api_internal("request.not_found", "request.not_found"))
}

fn retry_unwrap_dispatch(request_id: Vec<u8>) -> Result<RequestOverview, ApiError> {
    let request_id = tx_id_from_bytes(request_id)
        .ok_or_else(|| api_invalid_argument("arg.request_id_invalid", "arg.request_id_invalid"))?;
    let requeued = with_state_mut(|state| {
        let Some(mut req) = state.unwrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        if req.status != UnwrapRequestStatus::DispatchFailed {
            return Err("request.retry_invalid_state".to_string());
        }
        req.status = UnwrapRequestStatus::Queued;
        req.error_code = None;
        req.updated_at = current_time_nanos();
        state.unwrap_requests.insert(request_id, req);
        let mut meta = *state.unwrap_dispatch_meta.get();
        let seq = meta.push();
        state.unwrap_dispatch_meta.set(meta);
        state.unwrap_dispatch_queue.insert(seq, request_id);
        Ok(())
    });
    requeued.map_err(|err| api_rejected(&err, &err))?;
    #[cfg(target_arch = "wasm32")]
    schedule_unwrap_dispatch();
    get_request(request_id.0.to_vec())
        .ok_or_else(|| api_internal("request.not_found", "request.not_found"))
}

fn request_error_view(code: &str) -> RequestErrorView {
    RequestErrorView {
        code: code.to_string(),
        message: code.to_string(),
    }
}

fn wrap_request_stage_to_view(stage: WrapRequestStage) -> RequestStageView {
    match stage {
        WrapRequestStage::FeePending => RequestStageView::FeePending,
        WrapRequestStage::FeeCollected => RequestStageView::FeeCollected,
        WrapRequestStage::PullPending => RequestStageView::PullPending,
        WrapRequestStage::Pulled => RequestStageView::Pulled,
        WrapRequestStage::MintSubmitting => RequestStageView::MintSubmitting,
        WrapRequestStage::MintSubmitted => RequestStageView::MintSubmitted,
        WrapRequestStage::Succeeded => RequestStageView::Succeeded,
        WrapRequestStage::Failed => RequestStageView::Failed,
        WrapRequestStage::Refunding => RequestStageView::Refunding,
        WrapRequestStage::Refunded => RequestStageView::Refunded,
    }
}

fn unwrap_request_stage_to_view(status: UnwrapRequestStatus) -> RequestStageView {
    match status {
        UnwrapRequestStatus::Queued => RequestStageView::Queued,
        UnwrapRequestStatus::Dispatching => RequestStageView::Dispatching,
        UnwrapRequestStatus::Dispatched => RequestStageView::Dispatched,
        UnwrapRequestStatus::DispatchFailed => RequestStageView::DispatchFailed,
    }
}

fn unwrap_request_status_to_request_status(
    status: UnwrapRequestStatus,
) -> (RequestStatus, RequestDispatchStatusView) {
    match status {
        UnwrapRequestStatus::Queued => (RequestStatus::Queued, RequestDispatchStatusView::Queued),
        UnwrapRequestStatus::Dispatching => (
            RequestStatus::Running,
            RequestDispatchStatusView::Dispatching,
        ),
        UnwrapRequestStatus::Dispatched => (
            RequestStatus::Succeeded,
            RequestDispatchStatusView::Dispatched,
        ),
        UnwrapRequestStatus::DispatchFailed => (
            RequestStatus::Failed,
            RequestDispatchStatusView::DispatchFailed,
        ),
    }
}

#[ic_cdk::update]
fn set_fee_policy(args: FeePolicyArgs) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    validate_set_fee_policy(&args)?;
    with_state_mut(|state| {
        state.wrap_fee_policy.set(FeePolicyStored {
            fee_ledger_canister: args.fee_ledger_canister.as_slice().to_vec(),
            cycle_fee_e8s: args.cycle_fee_e8s,
            gas_price_buffer_bps: args.gas_price_buffer_bps,
        });
    });
    Ok(())
}

#[ic_cdk::update]
fn set_allowed_assets(assets: Vec<Principal>) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    validate_allowed_assets(&assets)?;
    with_state_mut(|state| {
        while let Some(entry) = state.wrap_allowed_assets.range(..).next() {
            let key = entry.key().clone();
            state.wrap_allowed_assets.remove(&key);
        }
        for asset in assets {
            state
                .wrap_allowed_assets
                .insert(asset.as_slice().to_vec(), 1);
        }
    });
    Ok(())
}

#[ic_cdk::update]
fn add_query_precompile_allowed_method(args: PrecompileAllowArgs) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    validate_query_precompile_allow_args(&args)?;
    let key = precompile_allow_key_for_principal(args.target, &args.method);
    with_state_mut(|state| {
        state.query_precompile_allowlist.insert(key, 1);
    });
    Ok(())
}

#[ic_cdk::update]
fn remove_query_precompile_allowed_method(args: PrecompileAllowArgs) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    validate_query_precompile_allow_args(&args)?;
    let key = precompile_allow_key_for_principal(args.target, &args.method);
    with_state_mut(|state| {
        state.query_precompile_allowlist.remove(&key);
    });
    Ok(())
}

#[ic_cdk::update]
fn add_update_precompile_allowed_method(args: PrecompileAllowArgs) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    validate_update_precompile_allow_args(&args)?;
    let key = precompile_allow_key_for_principal(args.target, &args.method);
    with_state_mut(|state| {
        state.icp_update_precompile_allowlist.insert(key, 1);
    });
    Ok(())
}

#[ic_cdk::update]
fn remove_update_precompile_allowed_method(args: PrecompileAllowArgs) -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    validate_update_precompile_allow_args(&args)?;
    let key = precompile_allow_key_for_principal(args.target, &args.method);
    with_state_mut(|state| {
        state.icp_update_precompile_allowlist.remove(&key);
    });
    Ok(())
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
    apply_wrap_config_from_init_args(&args);
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
    let data_plane_enabled = reject_write_reason().is_none();
    reset_mining_schedule_after_upgrade();
    restore_unwrap_dispatch_after_upgrade(data_plane_enabled);
    restore_icp_update_dispatch_after_upgrade(data_plane_enabled);
    restore_wrap_worker_after_upgrade(data_plane_enabled);
    if data_plane_enabled {
        schedule_mining();
    }
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

fn restore_unwrap_dispatch_after_upgrade(schedule_workers: bool) {
    // upgrade後は timer 実体が失われるため、永続化済みの unwrap queue を再接続する。
    if recover_unwrap_dispatch_state_after_upgrade(current_time_nanos()) && schedule_workers {
        schedule_unwrap_dispatch();
    }
}

fn restore_icp_update_dispatch_after_upgrade(schedule_workers: bool) {
    // update intentもpost-commit副作用なので、upgrade後は永続queueを再接続する。
    if recover_icp_update_dispatch_state_after_upgrade(current_time_nanos()) && schedule_workers {
        schedule_icp_update_dispatch();
    }
}

fn restore_wrap_worker_after_upgrade(schedule_workers: bool) {
    // upgrade後は timer 実体が失われるため、永続化済みの wrap queue を再接続する。
    if recover_wrap_worker_state_after_upgrade() && schedule_workers {
        schedule_wrap_worker();
    }
}

#[ic_cdk::update]
fn repair_stale_wrap_operations() -> Result<(), String> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(reason);
    }
    require_control_plane_write()?;
    repair_stale_operations(current_time_nanos());
    Ok(())
}

fn repair_stale_operations(now: u64) {
    settle_submitted_wrap_mint_receipts(now);
    let (wrap_needs_schedule, unwrap_needs_schedule, icp_update_needs_schedule) =
        with_state_mut(|state| {
            let cutoff = now.saturating_sub(STALE_OPERATION_NANOS);
            let mut wrap_requeue = Vec::new();
            let mut unwrap_requeue = Vec::new();

            let wrap_items: Vec<_> = state
                .wrap_requests
                .iter()
                .map(|entry| (*entry.key(), entry.value().clone()))
                .collect();
            for (request_id, mut req) in wrap_items {
                if req.result.withdraw_in_progress && req.result.updated_at <= cutoff {
                    req.result.withdraw_in_progress = false;
                    req.result.stage = WrapRequestStage::Failed;
                    req.result.updated_at = now;
                    let _ = sanitize_wrap_request(req.clone()).map(|clean| {
                        state.wrap_requests.insert(request_id, clean);
                    });
                    continue;
                }
                if req.result.status == StoredRequestStatus::Running
                    && req.result.updated_at <= cutoff
                    && req.result.mint_tx_id.is_none()
                    && req.result.mint_submit_status != MintSubmitStatus::Submitted
                {
                    req.result.status = StoredRequestStatus::Queued;
                    req.result.updated_at = now;
                    let _ = sanitize_wrap_request(req.clone()).map(|clean| {
                        state.wrap_requests.insert(request_id, clean);
                    });
                    wrap_requeue.push(request_id);
                }
            }

            let unwrap_items: Vec<_> = state
                .unwrap_requests
                .iter()
                .map(|entry| (*entry.key(), entry.value().clone()))
                .collect();
            for (request_id, mut req) in unwrap_items {
                if req.status == UnwrapRequestStatus::Dispatching && req.updated_at <= cutoff {
                    req.status = UnwrapRequestStatus::Queued;
                    req.updated_at = now;
                    if req.transfer_created_at_time == 0 {
                        req.transfer_created_at_time = now;
                    }
                    state.unwrap_requests.insert(request_id, req);
                    unwrap_requeue.push(request_id);
                }
            }

            let update_items: Vec<_> = state
                .icp_update_requests
                .iter()
                .map(|entry| (*entry.key(), entry.value().clone()))
                .collect();
            for (request_id, mut req) in update_items {
                if req.status == IcpUpdateRequestStatus::Dispatching && req.updated_at <= cutoff {
                    req.status = IcpUpdateRequestStatus::DispatchUncertain;
                    req.updated_at = now;
                    req.error_code = Some("ic_update.dispatch_uncertain".to_string());
                    state.icp_update_requests.insert(request_id, req);
                }
            }

            let mut wrap_queued = BTreeSet::new();
            for entry in state.wrap_queue.iter() {
                wrap_queued.insert(entry.value());
            }
            let mut wrap_meta = *state.wrap_queue_meta.get();
            for request_id in wrap_requeue {
                if wrap_queued.insert(request_id) {
                    let seq = wrap_meta.push();
                    state.wrap_queue.insert(seq, request_id);
                }
            }
            state.wrap_queue_meta.set(wrap_meta);

            let mut unwrap_queued = BTreeSet::new();
            for entry in state.unwrap_dispatch_queue.iter() {
                unwrap_queued.insert(entry.value());
            }
            let mut unwrap_meta = *state.unwrap_dispatch_meta.get();
            for request_id in unwrap_requeue {
                if unwrap_queued.insert(request_id) {
                    let seq = unwrap_meta.push();
                    state.unwrap_dispatch_queue.insert(seq, request_id);
                }
            }
            state.unwrap_dispatch_meta.set(unwrap_meta);

            (
                !state.wrap_queue.is_empty(),
                !state.unwrap_dispatch_queue.is_empty(),
                !state.icp_update_dispatch_queue.is_empty(),
            )
        });
    #[cfg(target_arch = "wasm32")]
    {
        if wrap_needs_schedule {
            schedule_wrap_worker();
        }
        if unwrap_needs_schedule {
            schedule_unwrap_dispatch();
        }
        if icp_update_needs_schedule {
            schedule_icp_update_dispatch();
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (
            wrap_needs_schedule,
            unwrap_needs_schedule,
            icp_update_needs_schedule,
        );
    }
}

fn recover_wrap_worker_state_after_upgrade() -> bool {
    with_state_mut(|state| {
        let mut queued_ids = BTreeSet::new();
        for item in state.wrap_queue.range(..) {
            queued_ids.insert(item.value());
        }

        let mut candidates = Vec::new();
        for item in state.wrap_requests.range(..) {
            let request_id = *item.key();
            let req = item.value();
            if req.gas_limit != 0
                && matches!(
                    item.value().result.status,
                    StoredRequestStatus::Queued | StoredRequestStatus::Running
                )
            {
                candidates.push(request_id);
            }
        }

        let mut meta = *state.wrap_queue_meta.get();
        for request_id in candidates {
            let mut should_queue = true;
            if let Some(mut req) = state.wrap_requests.get(&request_id) {
                if req.result.status == StoredRequestStatus::Running {
                    if req.result.mint_tx_id.is_some()
                        || req.result.mint_submit_status == MintSubmitStatus::Submitted
                    {
                        should_queue = false;
                    } else {
                        req.result.status = StoredRequestStatus::Queued;
                    }
                    state.wrap_requests.insert(request_id, req);
                }
            }
            if should_queue && queued_ids.insert(request_id) {
                let seq = meta.push();
                state.wrap_queue.insert(seq, request_id);
            }
        }
        state.wrap_queue_meta.set(meta);
        !state.wrap_queue.is_empty()
    })
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

fn recover_icp_update_dispatch_state_after_upgrade(now: u64) -> bool {
    with_state_mut(|state| {
        let mut queued_ids = BTreeSet::new();
        for item in state.icp_update_dispatch_queue.range(..) {
            queued_ids.insert(item.value());
        }

        let mut candidates = Vec::new();
        let mut uncertain = Vec::new();
        for item in state.icp_update_requests.range(..) {
            let request_id = *item.key();
            let mut req = item.value().clone();
            match req.status {
                IcpUpdateRequestStatus::Queued => {
                    if !queued_ids.contains(&request_id) {
                        candidates.push((request_id, req));
                    }
                }
                IcpUpdateRequestStatus::Dispatching => {
                    req.status = IcpUpdateRequestStatus::DispatchUncertain;
                    req.updated_at = now;
                    req.error_code = Some("ic_update.dispatch_uncertain".to_string());
                    uncertain.push((request_id, req));
                }
                IcpUpdateRequestStatus::Dispatched
                | IcpUpdateRequestStatus::DispatchFailed
                | IcpUpdateRequestStatus::DispatchUncertain => {}
            }
        }

        for (request_id, req) in uncertain {
            state.icp_update_requests.insert(request_id, req);
        }

        if candidates.is_empty() {
            return !state.icp_update_dispatch_queue.is_empty();
        }

        let mut meta = *state.icp_update_dispatch_meta.get();
        for (request_id, req) in candidates {
            state.icp_update_requests.insert(request_id, req);
            if queued_ids.contains(&request_id) {
                continue;
            }
            let seq = meta.push();
            state.icp_update_dispatch_queue.insert(seq, request_id);
            queued_ids.insert(request_id);
        }
        state.icp_update_dispatch_meta.set(meta);
        !state.icp_update_dispatch_queue.is_empty()
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
        method: "add_query_precompile_allowed_method",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "add_update_precompile_allowed_method",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "icrc21_canister_call_consent_message",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
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
        method: "credit_native_deposit",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "dispatch_native_withdrawal_request",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "dispatch_unwrap_request",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "quote_native_withdrawal",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "remove_query_precompile_allowed_method",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "remove_update_precompile_allowed_method",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "recover_failed_wrap",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "repair_stale_wrap_operations",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "retry_native_deposit",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "retry_native_withdrawal",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "retry_request",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_allowed_assets",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "set_fee_policy",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "add_query_precompile_allowed_method",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "remove_query_precompile_allowed_method",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "submit_native_deposit",
        payload_limit: INSPECT_MANAGE_PAYLOAD_LIMIT,
    },
    InspectMethodPolicy {
        method: "submit_wrap_request",
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

// inspect_message は ingress の事前拒否だけに使う。update body 側の
// controller check を本来の access control として維持する。
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
    submit_ic_tx_internal(
        ic_cdk::api::msg_caller().as_slice().to_vec(),
        "submit_ic_tx",
        tx,
    )
}

fn submit_ic_tx_internal(
    caller_principal: Vec<u8>,
    code: &'static str,
    tx: evm_core::tx_decode::IcSyntheticTxInput,
) -> Result<Vec<u8>, SubmitTxError> {
    submit_ic_tx_internal_with_canister(
        caller_principal,
        ic_cdk::api::canister_self().as_slice().to_vec(),
        code,
        tx,
    )
}

fn submit_ic_tx_internal_with_canister(
    caller_principal: Vec<u8>,
    canister_id: Vec<u8>,
    code: &'static str,
    tx: evm_core::tx_decode::IcSyntheticTxInput,
) -> Result<Vec<u8>, SubmitTxError> {
    submit_ic_tx_internal_with_canister_and_scheduler(
        caller_principal,
        canister_id,
        code,
        tx,
        schedule_mining,
    )
}

fn submit_ic_tx_internal_with_canister_and_scheduler(
    caller_principal: Vec<u8>,
    canister_id: Vec<u8>,
    code: &'static str,
    tx: evm_core::tx_decode::IcSyntheticTxInput,
    schedule_after_submit: fn(),
) -> Result<Vec<u8>, SubmitTxError> {
    let out = ic_evm_rpc::submit_tx_in_with_code(
        chain::TxIn::IcSynthetic {
            caller_principal,
            canister_id,
            tx,
        },
        code,
    );
    if out.is_ok() {
        schedule_after_submit();
    }
    out
}

#[ic_cdk::update]
fn credit_native_deposit(
    request_id: Vec<u8>,
    recipient: Vec<u8>,
    amount_wei: Nat,
) -> Result<(), ApiError> {
    if let Some(reason) = reject_anonymous_update() {
        return Err(api_rejected(&reason, &reason));
    }
    reject_data_plane_write()?;
    let caller = msg_caller();
    if caller != current_wrap_canister_id() {
        return Err(api_rejected(
            "auth.wrap_canister_required",
            "auth.wrap_canister_required",
        ));
    }
    if request_id.len() != 32 {
        return Err(api_invalid_argument(
            "arg.request_id_invalid",
            "arg.request_id_invalid",
        ));
    }
    if recipient.len() != 20 {
        return Err(api_invalid_argument(
            "arg.recipient_invalid",
            "arg.recipient_invalid",
        ));
    }
    let amount = nat_to_fixed_be::<32>(&amount_wei).ok_or_else(|| {
        api_invalid_argument("arg.amount_out_of_range", "arg.amount_out_of_range")
    })?;
    let mut request = [0u8; 32];
    request.copy_from_slice(&request_id);
    let mut to = [0u8; 20];
    to.copy_from_slice(&recipient);
    credit_native_deposit_internal(request, to, amount)
}

fn credit_native_deposit_internal(
    request: [u8; 32],
    to: [u8; 20],
    amount: [u8; 32],
) -> Result<(), ApiError> {
    chain::credit_native_deposit(request, to, amount).map_err(|err| match err {
        chain::ChainError::TxAlreadySeen => api_rejected(
            "native_deposit.idempotency_mismatch",
            "native_deposit.idempotency_mismatch",
        ),
        chain::ChainError::MintOverflow => api_rejected(
            "native_deposit.mint_overflow",
            "native_deposit.mint_overflow",
        ),
        other => api_internal("native_deposit.credit_failed", &format!("{other:?}")),
    })
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

fn api_rejected(code: &str, message: &str) -> ApiError {
    ApiError::Rejected(ApiErrorDetail {
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

#[ic_cdk::query(composite = true)]
async fn rpc_eth_call_object_with_query_precompile(
    call: RpcCallObjectView,
) -> Result<RpcCallResultView, RpcErrorView> {
    ic_evm_rpc::rpc_eth_call_object_async(call, resolve_icp_query_precompile).await
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

async fn resolve_icp_query_precompile(
    request: evm_core::kasane_precompiles::IcpQueryRequest,
) -> Result<Vec<u8>, String> {
    if !is_query_precompile_allowed(&request.target, &request.method) {
        return Err("ic_query.allowlist_miss".to_string());
    }
    let target = Principal::from_slice(&request.target);
    let response = Call::bounded_wait(target, &request.method)
        .take_raw_args(request.arg)
        .change_timeout(1)
        .await
        .map_err(|err| format!("ic_query.call_failed:{err}"))?;
    let bytes = response.into_bytes();
    if bytes.len() > MAX_RETURN_DATA {
        return Err("ic_query.response_too_large".to_string());
    }
    Ok(bytes)
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
        let avg = summary.txs.checked_div(summary.blocks).unwrap_or(0);
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
    evm_core::kasane_precompiles::precompile_profile_snapshot()
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
    evm_core::kasane_precompiles::clear_precompile_profile();
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

#[ic_cdk::query]
fn get_unwrap_request_ids_by_eth_tx_hash(eth_tx_hash: Vec<u8>) -> Vec<Vec<u8>> {
    let Some(tx) = ic_evm_rpc::rpc_eth_get_transaction_by_eth_hash(eth_tx_hash) else {
        return Vec::new();
    };
    get_unwrap_request_ids_by_tx_id(tx.hash)
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

fn reject_data_plane_write() -> Result<(), ApiError> {
    if let Some(reason) = reject_write_reason() {
        return Err(api_rejected(&reason, &reason));
    }
    Ok(())
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
    repair_stale_called: bool,
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
    let repair_stale_called = schedule_mining_called;
    if repair_stale_called {
        repair_stale_operations(current_time_nanos());
    }
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
        repair_stale_called,
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
                record_icp_update_requests_from_block(&outcome.block.tx_ids);
                settle_submitted_wrap_mint_receipts(current_time_nanos());
                schedule_unwrap_dispatch();
                schedule_icp_update_dispatch();
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppliedUnwrapDispatchOutcome {
    status: UnwrapRequestStatus,
    ledger_tx_id: Option<Vec<u8>>,
    error_code: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppliedIcpUpdateDispatchOutcome {
    status: IcpUpdateRequestStatus,
    reply: Option<Vec<u8>>,
    error_code: Option<String>,
}

fn record_unwrap_requests_from_block(tx_ids: &[TxId]) {
    for tx_id in tx_ids {
        let Some(receipt) = chain::get_receipt(tx_id) else {
            continue;
        };
        for (log_index, log) in receipt.logs.iter().enumerate() {
            if let Some(intent) = native_withdraw_intent_from_log(log) {
                let Some(request_id) = derive_log_request_id(tx_id, log_index) else {
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
                            asset_id: NATIVE_WITHDRAW_ASSET_MARKER.to_vec(),
                            amount: intent.amount_e8s,
                            recipient: intent.recipient,
                            status: UnwrapRequestStatus::Queued,
                            ledger_tx_id: None,
                            error_code: None,
                            updated_at: now,
                            transfer_created_at_time: 0,
                        },
                    );
                    let mut meta = *state.unwrap_dispatch_meta.get();
                    let seq = meta.push();
                    state.unwrap_dispatch_meta.set(meta);
                    state.unwrap_dispatch_queue.insert(seq, request_id);
                });
                continue;
            }
            let Some(intent) = unwrap_intent_from_log(log) else {
                continue;
            };
            let Some(request_id) = derive_log_request_id(tx_id, log_index) else {
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
                        transfer_created_at_time: 0,
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

fn record_icp_update_requests_from_block(tx_ids: &[TxId]) {
    for tx_id in tx_ids {
        let Some(receipt) = chain::get_receipt(tx_id) else {
            continue;
        };
        let Some(envelope) = chain::get_tx_envelope(tx_id) else {
            continue;
        };
        let caller = envelope.caller_evm.unwrap_or([0u8; 20]);
        let Ok(decoded) = decode_tx_view(envelope.kind, caller, &envelope.raw) else {
            continue;
        };
        let ic_caller = if envelope.kind == TxKind::IcSynthetic {
            Some(envelope.caller_principal.clone())
        } else {
            None
        };
        for (log_index, log) in receipt.logs.iter().enumerate() {
            let Some(intent) = icp_update_intent_from_log(log) else {
                continue;
            };
            let Some(request_id) = derive_log_request_id(tx_id, log_index) else {
                continue;
            };
            with_state_mut(|state| {
                if state.icp_update_requests.get(&request_id).is_some() {
                    return;
                }
                let now = current_time_nanos();
                state.icp_update_requests.insert(
                    request_id,
                    IcpUpdateDispatchRequest {
                        target: intent.target.clone(),
                        method: intent.method.clone(),
                        arg: intent.arg.clone(),
                        request_id,
                        tx_id: *tx_id,
                        block_number: receipt.block_number,
                        tx_index: receipt.tx_index,
                        log_index: u32::try_from(log_index).unwrap_or(u32::MAX),
                        tx_kind: envelope.kind,
                        evm_sender: decoded.from,
                        ic_caller: ic_caller.clone(),
                        status: IcpUpdateRequestStatus::Queued,
                        reply: None,
                        error_code: None,
                        updated_at: now,
                        call_started_at_time: 0,
                    },
                );
                let mut meta = *state.icp_update_dispatch_meta.get();
                let seq = meta.push();
                state.icp_update_dispatch_meta.set(meta);
                state.icp_update_dispatch_queue.insert(seq, request_id);
                trim_icp_update_requests(state);
            });
        }
    }
}

fn trim_icp_update_requests(state: &mut evm_db::stable_state::StableState) {
    let len = usize::try_from(state.icp_update_requests.len()).unwrap_or(usize::MAX);
    if len <= MAX_ICP_UPDATE_REQUESTS {
        return;
    }
    let mut completed = state
        .icp_update_requests
        .iter()
        .filter_map(|entry| {
            let status = entry.value().status;
            if matches!(
                status,
                IcpUpdateRequestStatus::Dispatched
                    | IcpUpdateRequestStatus::DispatchFailed
                    | IcpUpdateRequestStatus::DispatchUncertain
            ) {
                Some((entry.value().updated_at, *entry.key()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    completed.sort_by_key(|(updated_at, request_id)| (*updated_at, request_id.0));

    let mut remaining = len;
    for (_, request_id) in completed {
        if remaining <= MAX_ICP_UPDATE_REQUESTS {
            break;
        }
        state.icp_update_requests.remove(&request_id);
        remove_icp_update_queue_entries(state, request_id);
        remaining = remaining.saturating_sub(1);
    }
}

fn remove_icp_update_queue_entries(
    state: &mut evm_db::stable_state::StableState,
    request_id: TxId,
) {
    let seqs = state
        .icp_update_dispatch_queue
        .iter()
        .filter_map(|entry| {
            if entry.value() == request_id {
                Some(*entry.key())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for seq in seqs {
        state.icp_update_dispatch_queue.remove(&seq);
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

fn schedule_icp_update_dispatch() {
    if ICP_UPDATE_DISPATCH_SCHEDULED
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    arm_icp_update_dispatch_timer();
}

fn arm_icp_update_dispatch_timer() {
    #[cfg(test)]
    ICP_UPDATE_DISPATCH_TIMER_ARMS.fetch_add(1, Ordering::SeqCst);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _tick = icp_update_dispatch_tick;
    }
    #[cfg(target_arch = "wasm32")]
    ic_cdk_timers::set_timer(
        std::time::Duration::from_millis(WRAP_DISPATCH_DELAY_MS),
        async move {
            icp_update_dispatch_tick().await;
        },
    );
}

#[cfg(test)]
fn reset_icp_update_dispatch_scheduler_for_tests(scheduled: bool) {
    ICP_UPDATE_DISPATCH_SCHEDULED.store(scheduled, Ordering::SeqCst);
    ICP_UPDATE_DISPATCH_TIMER_ARMS.store(0, Ordering::SeqCst);
}

#[cfg(test)]
fn icp_update_dispatch_scheduler_state_for_tests() -> (bool, u64) {
    (
        ICP_UPDATE_DISPATCH_SCHEDULED.load(Ordering::SeqCst),
        ICP_UPDATE_DISPATCH_TIMER_ARMS.load(Ordering::SeqCst),
    )
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn schedule_wrap_worker() {
    let should_schedule = !WRAP_WORKER_SCHEDULED.swap(true, Ordering::SeqCst);
    if !should_schedule {
        return;
    }
    ic_cdk_timers::set_timer(std::time::Duration::from_millis(50), async move {
        wrap_worker_tick().await;
    });
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
        if req.transfer_created_at_time == 0 {
            req.transfer_created_at_time = now;
        }
        state.unwrap_requests.insert(request_id, req.clone());
        Ok(Some((request_id, req)))
    });
    out
}

fn pop_next_icp_update_request(
    now: u64,
) -> Result<Option<(TxId, IcpUpdateDispatchRequest)>, String> {
    let out = with_state_mut(|state| {
        let mut meta = *state.icp_update_dispatch_meta.get();
        let seq = match meta.pop() {
            Some(v) => v,
            None => {
                state.icp_update_dispatch_meta.set(meta);
                return Ok(None);
            }
        };
        state.icp_update_dispatch_meta.set(meta);

        let Some(request_id) = state.icp_update_dispatch_queue.get(&seq) else {
            return Err(format!("ic_update.dispatch.queue_missing:seq={seq}"));
        };
        state.icp_update_dispatch_queue.remove(&seq);
        let Some(mut req) = state.icp_update_requests.get(&request_id) else {
            return Err(format!(
                "ic_update.dispatch.request_missing:request_id={:?}",
                request_id.0
            ));
        };
        if is_decode_failed_icp_update_request(&req) {
            req.updated_at = now;
            req.status = IcpUpdateRequestStatus::DispatchFailed;
            req.reply = None;
            req.error_code = Some(ICP_UPDATE_DECODE_FAILURE_CODE.to_string());
            state.icp_update_requests.insert(request_id, req);
            return Err(format!(
                "ic_update.dispatch.quarantined:request_id={:?}:reason={ICP_UPDATE_DECODE_FAILURE_CODE}",
                request_id.0
            ));
        }
        req.status = IcpUpdateRequestStatus::Dispatching;
        req.updated_at = now;
        if req.call_started_at_time == 0 {
            req.call_started_at_time = now;
        }
        state.icp_update_requests.insert(request_id, req.clone());
        Ok(Some((request_id, req)))
    });
    out
}

fn is_decode_failed_unwrap_request(req: &UnwrapDispatchRequest) -> bool {
    req.error_code.as_deref() == Some(UNWRAP_DECODE_FAILURE_CODE)
}

fn is_decode_failed_icp_update_request(req: &IcpUpdateDispatchRequest) -> bool {
    req.error_code.as_deref() == Some(ICP_UPDATE_DECODE_FAILURE_CODE)
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

        finalize_unwrap_dispatch_attempt(
            request_id,
            current_time_nanos(),
            dispatch_unwrap_request_internal(request_id, req).await,
        );

        if with_state(|state| !state.unwrap_dispatch_queue.is_empty()) {
            schedule_unwrap_dispatch();
        }
        break;
    }
}

async fn icp_update_dispatch_tick() {
    loop {
        let next = pop_next_icp_update_request(current_time_nanos());
        let Some((request_id, req)) = (match next {
            Ok(v) => v,
            Err(err) => {
                if err.starts_with("ic_update.dispatch.quarantined:") {
                    warn!(error = err, "icp_update_dispatch_tick quarantined request");
                } else {
                    error!(
                        error = err,
                        "icp_update_dispatch_tick skipped corrupted queue entry"
                    );
                }
                continue;
            }
        }) else {
            finish_icp_update_dispatch_tick();
            break;
        };

        finalize_icp_update_dispatch_attempt(
            request_id,
            current_time_nanos(),
            dispatch_icp_update_request_internal(req).await,
        );

        complete_icp_update_dispatch_tick();
        break;
    }
}

fn complete_icp_update_dispatch_tick() {
    if with_state(|state| !state.icp_update_dispatch_queue.is_empty()) {
        arm_icp_update_dispatch_timer();
    } else {
        finish_icp_update_dispatch_tick();
    }
}

fn finish_icp_update_dispatch_tick() {
    ICP_UPDATE_DISPATCH_SCHEDULED.store(false, Ordering::SeqCst);
    if with_state(|state| !state.icp_update_dispatch_queue.is_empty()) {
        schedule_icp_update_dispatch();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WrapMintReceiptSettlement {
    Succeeded,
    Failed,
}

fn submitted_wrap_mint_receipt_candidates() -> Vec<(TxId, TxId)> {
    with_state(|state| {
        state
            .wrap_requests
            .iter()
            .filter_map(|entry| {
                let req = entry.value();
                if req.gas_limit == 0
                    || req.result.status != StoredRequestStatus::Running
                    || req.result.mint_submit_status != MintSubmitStatus::Submitted
                {
                    return None;
                }
                let mint_tx_id = tx_id_from_bytes(req.result.mint_tx_id.clone()?)?;
                Some((*entry.key(), mint_tx_id))
            })
            .collect()
    })
}

fn settle_submitted_wrap_mint_receipts(now: u64) -> u64 {
    let settlements = submitted_wrap_mint_receipt_candidates()
        .into_iter()
        .filter_map(|(request_id, mint_tx_id)| {
            let receipt = chain::get_receipt(&mint_tx_id)?;
            let settlement = if receipt.status == 1 {
                WrapMintReceiptSettlement::Succeeded
            } else {
                WrapMintReceiptSettlement::Failed
            };
            Some((request_id, settlement))
        })
        .collect::<Vec<_>>();
    let settled = u64::try_from(settlements.len()).unwrap_or(u64::MAX);
    if settlements.is_empty() {
        return 0;
    }
    with_state_mut(|state| {
        for (request_id, settlement) in settlements {
            let Some(mut req) = state.wrap_requests.get(&request_id) else {
                continue;
            };
            if req.result.status != StoredRequestStatus::Running
                || req.result.mint_submit_status != MintSubmitStatus::Submitted
            {
                continue;
            }
            req.result.updated_at = now;
            match settlement {
                WrapMintReceiptSettlement::Succeeded => {
                    req.result.status = StoredRequestStatus::Succeeded;
                    req.result.stage = WrapRequestStage::Succeeded;
                    req.result.error_code = None;
                    req.result.mint_failed_recoverable = false;
                }
                WrapMintReceiptSettlement::Failed => {
                    req.result.status = StoredRequestStatus::Failed;
                    req.result.stage = WrapRequestStage::Failed;
                    req.result.error_code = Some("wrap.mint_receipt_failed".to_string());
                    req.result.mint_failed_recoverable = true;
                }
            }
            if let Ok(req) = sanitize_wrap_request(req) {
                state.wrap_requests.insert(request_id, req);
            }
        }
    });
    settled
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
async fn wrap_worker_tick() {
    WRAP_WORKER_SCHEDULED.store(false, Ordering::SeqCst);
    while let Some(request_id) = dequeue_wrap_request() {
        let req = with_state_mut(|state| {
            let mut req = state.wrap_requests.get(&request_id)?;
            req.result.status = StoredRequestStatus::Running;
            req.result.updated_at = current_time_nanos();
            state.wrap_requests.insert(request_id, req.clone());
            Some(req)
        });
        let Some(req) = req else {
            continue;
        };
        let outcome = execute_wrap_request(request_id, req).await;
        apply_wrap_execution_outcome(request_id, outcome);
        if with_state(|state| !state.wrap_queue_meta.get().is_empty()) {
            schedule_wrap_worker();
        }
        break;
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
struct WrapExecutionOutcome {
    status: StoredRequestStatus,
    pull_ledger_tx_id: Option<Vec<u8>>,
    mint_tx_id: Option<Vec<u8>>,
    error_code: Option<String>,
    mint_failed_recoverable: bool,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
async fn execute_wrap_request(
    request_id: TxId,
    req: evm_db::chain_data::WrapStoredRequest,
) -> WrapExecutionOutcome {
    let caller = match principal_from_stored_bytes(&req.caller) {
        Ok(caller) => caller,
        Err(code) => return wrap_failed(None, code, false),
    };
    let asset = match principal_from_stored_bytes(&req.asset_id) {
        Ok(asset) => asset,
        Err(code) => return wrap_failed(None, code, false),
    };
    let pull_ledger_tx_id = match req.result.pull_ledger_tx_id.clone() {
        Some(tx_id) => tx_id,
        None => {
            mark_wrap_stage(
                request_id,
                WrapRequestStage::PullPending,
                StoredRequestStatus::Running,
            );
            let pull = attempt_icrc2_transfer_from(
                caller,
                asset,
                Nat(BigUint::from_bytes_be(&req.amount)),
                request_memo(request_id, TransferMemoKind::Pull),
                req.pull_created_at_time,
            )
            .await;
            match pull {
                Ok(tx_id) => {
                    if let Err(code) = record_wrap_pull_success(request_id, tx_id.clone()) {
                        return wrap_failed(None, code, false);
                    }
                    tx_id
                }
                Err(code) => return wrap_failed(None, code, false),
            }
        }
    };
    let mint = submit_mint_tx_internal(request_id, &req).await;
    match mint {
        Ok(mint_tx_id) => WrapExecutionOutcome {
            status: StoredRequestStatus::Running,
            pull_ledger_tx_id: Some(pull_ledger_tx_id),
            mint_tx_id: Some(mint_tx_id),
            error_code: None,
            mint_failed_recoverable: false,
        },
        Err(code) => wrap_failed(Some(pull_ledger_tx_id), code, true),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn wrap_failed(
    pull_ledger_tx_id: Option<Vec<u8>>,
    code: String,
    mint_failed_recoverable: bool,
) -> WrapExecutionOutcome {
    WrapExecutionOutcome {
        status: StoredRequestStatus::Failed,
        pull_ledger_tx_id,
        mint_tx_id: None,
        error_code: Some(code),
        mint_failed_recoverable,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn mark_wrap_stage(request_id: TxId, stage: WrapRequestStage, status: StoredRequestStatus) {
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return;
        };
        req.result.status = status;
        req.result.stage = stage;
        req.result.updated_at = current_time_nanos();
        if let Ok(req) = sanitize_wrap_request(req) {
            state.wrap_requests.insert(request_id, req);
        }
    });
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn record_wrap_pull_success(request_id: TxId, pull_ledger_tx_id: Vec<u8>) -> Result<(), String> {
    let pull_ledger_tx_id = validated_ledger_tx_id(pull_ledger_tx_id)?;
    let out = with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        req.result.pull_ledger_tx_id = Some(pull_ledger_tx_id);
        req.result.stage = WrapRequestStage::Pulled;
        req.result.updated_at = current_time_nanos();
        req.result.error_code = None;
        let req = sanitize_wrap_request(req)?;
        state.wrap_requests.insert(request_id, req);
        Ok(())
    });
    out?;
    Ok(())
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
async fn submit_mint_tx_internal(
    request_id: TxId,
    req: &evm_db::chain_data::WrapStoredRequest,
) -> Result<Vec<u8>, String> {
    if let Some(tx_id) = req.result.mint_tx_id.clone() {
        return Ok(tx_id);
    }
    let factory = expected_wrap_factory_address()?;
    let token_decimals = fetch_asset_decimals(&req.asset_id).await?;
    let data = encode_factory_mint_for_asset_call_data(
        &req.asset_id,
        token_decimals,
        &req.evm_recipient,
        &req.amount,
    )?;
    let wrap_evm = hash::derive_evm_address_from_principal(ic_cdk::api::canister_self().as_slice())
        .map_err(|_| "wrap.evm_address_derivation_failed".to_string())?;
    let nonce = req
        .result
        .mint_nonce
        .unwrap_or_else(|| chain::expected_nonce_for_sender_view(wrap_evm));
    record_wrap_mint_submitting(request_id, nonce)?;
    let charged_gas_price_wei = req.result.charged_gas_price_wei.unwrap_or(0);
    let suggested_priority_fee_wei =
        ic_evm_rpc::rpc_eth_max_priority_fee_per_gas().map_err(|err| {
            let code = err
                .error_prefix
                .unwrap_or_else(|| format!("rpc.error.{}", err.code));
            format!("fee.priority_failed:{code}:{}", err.message)
        })?;
    let priority = suggested_priority_fee_wei.min(charged_gas_price_wei);
    let mut to = [0u8; 20];
    to.copy_from_slice(&factory);
    let tx = evm_core::tx_decode::IcSyntheticTxInput {
        to: Some(to),
        value: [0u8; 32],
        gas_limit: req.gas_limit,
        max_fee_per_gas: charged_gas_price_wei,
        max_priority_fee_per_gas: priority,
        nonce,
        data,
    };
    let tx_id = derive_ic_synthetic_tx_id(
        ic_cdk::api::canister_self().as_slice(),
        ic_cdk::api::canister_self().as_slice(),
        &tx,
        wrap_evm,
    );
    let submit = submit_ic_tx_internal(
        ic_cdk::api::canister_self().as_slice().to_vec(),
        "wrap_mint",
        tx,
    );
    match submit {
        Ok(tx_id) => {
            record_wrap_mint_submitted(request_id, tx_id.clone())?;
            Ok(tx_id)
        }
        Err(err) => {
            if is_duplicate_mint_submit_error(&err) && chain::get_tx_loc(&tx_id).is_some() {
                let tx_id = tx_id.0.to_vec();
                record_wrap_mint_submitted(request_id, tx_id.clone())?;
                return Ok(tx_id);
            }
            Err(format!(
                "evm_gateway.submit_failed:{}",
                submit_error_to_code(err)
            ))
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn derive_ic_synthetic_tx_id(
    caller_principal: &[u8],
    canister_id: &[u8],
    tx: &evm_core::tx_decode::IcSyntheticTxInput,
    caller_evm: [u8; 20],
) -> TxId {
    let tx_bytes = evm_core::tx_decode::encode_ic_synthetic_input(tx);
    TxId(hash::stored_tx_id(
        TxKind::IcSynthetic,
        &tx_bytes,
        Some(caller_evm),
        Some(canister_id),
        Some(caller_principal),
    ))
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn is_duplicate_mint_submit_error(err: &SubmitTxError) -> bool {
    matches!(
        err,
        SubmitTxError::Rejected(code)
            if code == "submit.tx_already_seen" || code == "submit.nonce_conflict"
    )
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn record_wrap_mint_submitting(request_id: TxId, nonce: u64) -> Result<(), String> {
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        req.result.mint_nonce = Some(nonce);
        req.result.mint_submit_status = MintSubmitStatus::Submitting;
        req.result.stage = WrapRequestStage::MintSubmitting;
        req.result.updated_at = current_time_nanos();
        let req = sanitize_wrap_request(req)?;
        state.wrap_requests.insert(request_id, req);
        Ok(())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn record_wrap_mint_submitted(request_id: TxId, tx_id: Vec<u8>) -> Result<(), String> {
    let tx_id = validated_ledger_tx_id(tx_id)?;
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("request.not_found".to_string());
        };
        req.result.mint_tx_id = Some(tx_id);
        req.result.mint_submitted_at_time = current_time_nanos();
        req.result.mint_submit_status = MintSubmitStatus::Submitted;
        req.result.stage = WrapRequestStage::MintSubmitted;
        req.result.updated_at = current_time_nanos();
        req.result.error_code = None;
        let req = sanitize_wrap_request(req)?;
        state.wrap_requests.insert(request_id, req);
        Ok(())
    })
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn submit_error_to_code(error: SubmitTxError) -> String {
    match error {
        SubmitTxError::InvalidArgument(code) => format!("invalid_argument:{code}"),
        SubmitTxError::Rejected(code) => format!("rejected:{code}"),
        SubmitTxError::Internal(code) => format!("internal:{code}"),
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn apply_wrap_execution_outcome(request_id: TxId, outcome: WrapExecutionOutcome) {
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return;
        };
        req.result.status = outcome.status;
        if outcome.pull_ledger_tx_id.is_some() {
            req.result.pull_ledger_tx_id = outcome.pull_ledger_tx_id;
        }
        if outcome.mint_tx_id.is_some() {
            req.result.mint_tx_id = outcome.mint_tx_id;
        }
        req.result.error_code = outcome.error_code.map(clamp_error_code);
        req.result.mint_failed_recoverable = outcome.mint_failed_recoverable;
        req.result.stage = match outcome.status {
            StoredRequestStatus::Succeeded => WrapRequestStage::Succeeded,
            StoredRequestStatus::Failed => WrapRequestStage::Failed,
            StoredRequestStatus::Running if req.result.mint_tx_id.is_some() => {
                WrapRequestStage::MintSubmitted
            }
            StoredRequestStatus::Running => WrapRequestStage::MintSubmitting,
            StoredRequestStatus::Queued => WrapRequestStage::FeeCollected,
        };
        req.result.updated_at = current_time_nanos();
        if let Ok(req) = sanitize_wrap_request(req) {
            state.wrap_requests.insert(request_id, req);
        }
    });
}

fn is_native_withdraw_dispatch_request(req: &UnwrapDispatchRequest) -> bool {
    req.asset_id.as_slice() == NATIVE_WITHDRAW_ASSET_MARKER
}

fn gross_transfer_receive_amount(
    amount: &Nat,
    fee: u128,
    code_prefix: &str,
) -> Result<Nat, String> {
    let gross_amount =
        nat_to_u128(amount).ok_or_else(|| format!("{code_prefix}.amount_out_of_range"))?;
    let receive_amount = gross_amount
        .checked_sub(fee)
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{code_prefix}.amount_not_above_fee"))?;
    Ok(Nat(BigUint::from(receive_amount)))
}

fn native_withdraw_gross_transfer_amount(amount: &Nat, fee: u128) -> Result<Nat, String> {
    gross_transfer_receive_amount(amount, fee, "native_withdraw")
}

async fn dispatch_unwrap_request_internal(
    request_id: TxId,
    req: UnwrapDispatchRequest,
) -> AppliedUnwrapDispatchOutcome {
    let ledger = if is_native_withdraw_dispatch_request(&req) {
        match current_native_ledger_canister() {
            Ok(ledger) => ledger,
            Err(code) => {
                return AppliedUnwrapDispatchOutcome {
                    status: UnwrapRequestStatus::DispatchFailed,
                    ledger_tx_id: None,
                    error_code: Some(code),
                };
            }
        }
    } else {
        match principal_from_stored_bytes(&req.asset_id) {
            Ok(ledger) => ledger,
            Err(code) => {
                return AppliedUnwrapDispatchOutcome {
                    status: UnwrapRequestStatus::DispatchFailed,
                    ledger_tx_id: None,
                    error_code: Some(code),
                };
            }
        }
    };
    let recipient = match principal_from_stored_bytes(&req.recipient) {
        Ok(recipient) => recipient,
        Err(code) => {
            return AppliedUnwrapDispatchOutcome {
                status: UnwrapRequestStatus::DispatchFailed,
                ledger_tx_id: None,
                error_code: Some(code),
            };
        }
    };
    let gross_amount = Nat(BigUint::from_bytes_be(&req.amount));
    let amount = if is_native_withdraw_dispatch_request(&req) {
        let fee = match fetch_icrc1_fee(ledger).await {
            Ok(fee) => fee,
            Err(code) => {
                return AppliedUnwrapDispatchOutcome {
                    status: UnwrapRequestStatus::DispatchFailed,
                    ledger_tx_id: None,
                    error_code: Some(code),
                };
            }
        };
        match native_withdraw_gross_transfer_amount(&gross_amount, fee) {
            Ok(amount) => amount,
            Err(code) => {
                return AppliedUnwrapDispatchOutcome {
                    status: UnwrapRequestStatus::DispatchFailed,
                    ledger_tx_id: None,
                    error_code: Some(code),
                };
            }
        }
    } else {
        gross_amount
    };
    match attempt_icrc1_transfer(
        ledger,
        recipient,
        amount,
        request_memo(request_id, TransferMemoKind::Unwrap),
        req.transfer_created_at_time,
    )
    .await
    {
        Ok(ledger_tx_id) => AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::Dispatched,
            ledger_tx_id: Some(ledger_tx_id),
            error_code: None,
        },
        Err(code) => AppliedUnwrapDispatchOutcome {
            status: UnwrapRequestStatus::DispatchFailed,
            ledger_tx_id: None,
            error_code: Some(code),
        },
    }
}

async fn dispatch_icp_update_request_internal(
    req: IcpUpdateDispatchRequest,
) -> AppliedIcpUpdateDispatchOutcome {
    if !is_update_precompile_allowed(&req.target, &req.method) {
        return AppliedIcpUpdateDispatchOutcome {
            status: IcpUpdateRequestStatus::DispatchFailed,
            reply: None,
            error_code: Some("ic_update.allowlist_miss".to_string()),
        };
    }
    let target = Principal::from_slice(&req.target);
    let envelope = icp_update_envelope(&req);
    let response = Call::bounded_wait(target, &req.method)
        .with_arg(envelope)
        .change_timeout(ICP_UPDATE_DISPATCH_TIMEOUT_SECONDS)
        .await;
    match response {
        Ok(response) => icp_update_success_outcome(response.into_bytes()),
        Err(err) => {
            let uncertain = is_icp_update_uncertain_call_error(&err);
            let detail = format!("{err}");
            let status = if uncertain {
                IcpUpdateRequestStatus::DispatchUncertain
            } else {
                IcpUpdateRequestStatus::DispatchFailed
            };
            AppliedIcpUpdateDispatchOutcome {
                status,
                reply: None,
                error_code: Some(format!("ic_update.call_failed:{detail}")),
            }
        }
    }
}

fn icp_update_envelope(req: &IcpUpdateDispatchRequest) -> IcpUpdateEnvelopeV1 {
    IcpUpdateEnvelopeV1 {
        version: 1,
        chain_id: CHAIN_ID,
        request_id: req.request_id.0.to_vec(),
        tx_id: req.tx_id.0.to_vec(),
        block_number: req.block_number,
        tx_index: req.tx_index,
        log_index: req.log_index,
        tx_kind: tx_kind_to_icp_update_view(req.tx_kind),
        evm_sender: req.evm_sender.to_vec(),
        ic_caller: req
            .ic_caller
            .as_ref()
            .map(|bytes| Principal::from_slice(bytes)),
        arg: req.arg.clone(),
    }
}

fn is_icp_update_uncertain_call_error(error: &CallFailed) -> bool {
    // bounded_wait timeout is surfaced as SysUnknown. CallPerformFailed means the
    // request was not enqueued, so it remains a deterministic dispatch failure.
    matches!(
        error,
        CallFailed::CallRejected(rejection)
            if rejection.reject_code().ok() == Some(RejectCode::SysUnknown)
    )
}

fn icp_update_success_outcome(reply: Vec<u8>) -> AppliedIcpUpdateDispatchOutcome {
    if reply.len() > MAX_RETURN_DATA {
        return AppliedIcpUpdateDispatchOutcome {
            status: IcpUpdateRequestStatus::Dispatched,
            reply: None,
            error_code: Some(ICP_UPDATE_REPLY_OMITTED_TOO_LARGE.to_string()),
        };
    }
    AppliedIcpUpdateDispatchOutcome {
        status: IcpUpdateRequestStatus::Dispatched,
        reply: Some(reply),
        error_code: None,
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
        req.ledger_tx_id = applied
            .ledger_tx_id
            .and_then(|tx_id| validated_ledger_tx_id(tx_id).ok());
        req.status = applied.status;
        req.error_code = applied.error_code.map(clamp_error_code);
        state.unwrap_requests.insert(request_id, req);
    });
}

fn finalize_icp_update_dispatch_attempt(
    request_id: TxId,
    now: u64,
    applied: AppliedIcpUpdateDispatchOutcome,
) {
    with_state_mut(|state| {
        let Some(mut req) = state.icp_update_requests.get(&request_id) else {
            return;
        };
        req.updated_at = now;
        req.reply = applied.reply;
        req.status = applied.status;
        req.error_code = applied.error_code.map(clamp_error_code);
        state.icp_update_requests.insert(request_id, req);
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

fn derive_log_request_id(tx_id: &TxId, log_index: usize) -> Option<TxId> {
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
            if unwrap_intent_from_log(log).is_none()
                && native_withdraw_intent_from_log(log).is_none()
            {
                return None;
            }
            derive_log_request_id(tx_id, log_index)
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

fn icp_update_request_status_to_view(status: IcpUpdateRequestStatus) -> RequestDispatchStatusView {
    match status {
        IcpUpdateRequestStatus::Queued => RequestDispatchStatusView::Queued,
        IcpUpdateRequestStatus::Dispatching => RequestDispatchStatusView::Dispatching,
        IcpUpdateRequestStatus::Dispatched => RequestDispatchStatusView::Dispatched,
        IcpUpdateRequestStatus::DispatchFailed => RequestDispatchStatusView::DispatchFailed,
        IcpUpdateRequestStatus::DispatchUncertain => RequestDispatchStatusView::DispatchUncertain,
    }
}

fn tx_kind_to_icp_update_view(kind: TxKind) -> IcpUpdateTxKindView {
    match kind {
        TxKind::EthSigned => IcpUpdateTxKindView::EthSigned,
        TxKind::IcSynthetic => IcpUpdateTxKindView::IcSynthetic,
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

#[ic_cdk::query]
fn icrc10_supported_standards() -> Vec<icrc21::StandardRecord> {
    icrc21::supported_standards()
}

#[ic_cdk::update]
async fn icrc21_canister_call_consent_message(
    request: icrc21::Icrc21ConsentMessageRequest,
) -> icrc21::Icrc21ConsentMessageResponse {
    icrc21::consent_message(request).await
}

ic_cdk::export_candid!();

// NOTE: build-time only; keep out of production surface area.
#[cfg(feature = "did-gen")]
pub fn export_did() -> String {
    __export_service()
}

#[cfg(test)]
mod tests;
