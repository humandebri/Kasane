//! where: wrap/vault canister
//! what: unwrap + wrap request queue workers
//! why: split asset execution from kasane core

use candid::{CandidType, Deserialize, Nat, Principal};
use ic_evm_rpc_types::{
    ApiError, ApiErrorDetail, RequestDispatchStatusView, RpcCallObjectView, RpcCallResultView,
    RpcErrorView,
};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap, StableCell, Storable};
use num_bigint::BigUint;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use tiny_keccak::{Hasher, Keccak};

#[cfg(target_arch = "wasm32")]
getrandom::register_custom_getrandom!(always_fail_getrandom);

#[cfg(target_arch = "wasm32")]
fn always_fail_getrandom(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}

const MEM_KASANE_CANISTER: MemoryId = MemoryId::new(0);
const MEM_REQUESTS: MemoryId = MemoryId::new(1);
const MEM_QUEUE: MemoryId = MemoryId::new(2);
const MEM_QUEUE_META: MemoryId = MemoryId::new(3);
const MEM_EVM_GATEWAY_CANISTER: MemoryId = MemoryId::new(4);
const MEM_WRAP_REQUESTS: MemoryId = MemoryId::new(5);
const MEM_WRAP_QUEUE: MemoryId = MemoryId::new(6);
const MEM_WRAP_QUEUE_META: MemoryId = MemoryId::new(7);
const MEM_FEE_POLICY: MemoryId = MemoryId::new(9);
const MEM_WRAP_EVM_CONFIG: MemoryId = MemoryId::new(10);
const PRINCIPAL_MAX_BYTES: usize = 29;
const AMOUNT_BYTES: usize = 32;
const EVM_ADDRESS_BYTES: usize = 20;
const MAX_LEDGER_TX_ID_BYTES: usize = 128;
const MAX_ERROR_CODE_BYTES: usize = 192;
const STORED_REQUEST_MAX_BYTES: u32 = 768;
const WRAP_STORED_REQUEST_MAX_BYTES: u32 = 768;
const FEE_POLICY_MAX_BYTES: u32 = 128;
const WRAP_EVM_CONFIG_MAX_BYTES: u32 = 32;
const DEFAULT_CYCLE_FEE_E8S: u64 = 1_000_000;
const DEFAULT_GAS_PRICE_BUFFER_BPS: u32 = 12_000;
const GAS_PRICE_DENOMINATOR_BPS: u128 = 10_000;
const WEI_PER_E8S: u128 = 10_000_000_000;
type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub kasane_canister: Principal,
    pub evm_gateway_canister: Principal,
    pub fee_ledger_canister: Principal,
    pub wrap_factory_address: Vec<u8>,
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

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct DispatchUnwrapRequestArgs {
    pub request_id: Vec<u8>,
    pub asset_id: Principal,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InsertRequestOutcome {
    Inserted(RequestId),
    AlreadyExists(RequestId),
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum RequestStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct RequestResult {
    pub status: RequestStatus,
    pub ledger_tx_id: Option<Vec<u8>>,
    pub error_code: Option<String>,
    #[serde(default)]
    pub dispatch_status: Option<RequestDispatchStatusView>,
    #[serde(default)]
    pub dispatch_error: Option<String>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WrapRequestResult {
    pub status: RequestStatus,
    pub pull_ledger_tx_id: Option<Vec<u8>>,
    pub mint_tx_id: Option<Vec<u8>>,
    pub error_code: Option<String>,
    #[serde(default)]
    pub withdrawn: bool,
    #[serde(default)]
    pub withdraw_ledger_tx_id: Option<Vec<u8>>,
    #[serde(default)]
    pub withdraw_error_code: Option<String>,
    #[serde(default)]
    pub withdraw_in_progress: bool,
    #[serde(default)]
    pub mint_failed_recoverable: bool,
    #[serde(default)]
    pub fee_ledger_tx_id: Option<Vec<u8>>,
    #[serde(default)]
    pub charged_fee_e8s: Option<u128>,
    #[serde(default)]
    pub charged_gas_price_wei: Option<u128>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct FeePolicyView {
    pub fee_ledger_canister: Principal,
    pub cycle_fee_e8s: u64,
    pub gas_price_buffer_bps: u32,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum RequestKind {
    Wrap,
    Unwrap,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub enum UnwrapReadiness {
    Ready,
    TokenNotDeployed,
    InsufficientBalance,
    InsufficientAllowance,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
pub struct RequestErrorView {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct RequestOverview {
    pub kind: RequestKind,
    pub request_id: Vec<u8>,
    pub status: RequestStatus,
    pub error: Option<RequestErrorView>,
    pub fee_ledger_tx_id: Option<Vec<u8>>,
    pub pull_ledger_tx_id: Option<Vec<u8>>,
    pub mint_tx_id: Option<Vec<u8>>,
    pub withdraw_ledger_tx_id: Option<Vec<u8>>,
    pub ledger_tx_id: Option<Vec<u8>>,
    pub dispatch_status: Option<RequestDispatchStatusView>,
    pub dispatch_error: Option<String>,
    pub charged_fee_e8s: Option<Nat>,
    pub charged_gas_price_wei: Option<Nat>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SetFeePolicyArgs {
    pub fee_ledger_canister: Principal,
    pub cycle_fee_e8s: u64,
    pub gas_price_buffer_bps: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct StoredRequest {
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    recipient: Vec<u8>,
    result: RequestResult,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct WrapStoredRequest {
    caller: Vec<u8>,
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    evm_recipient: Vec<u8>,
    gas_limit: u64,
    result: WrapRequestResult,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct FeePolicyStored {
    fee_ledger_canister: Vec<u8>,
    cycle_fee_e8s: u64,
    gas_price_buffer_bps: u32,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct WrapEvmConfigStored {
    wrap_factory_address: Vec<u8>,
}

#[derive(Clone, Debug)]
struct FeeCharge {
    ledger_tx_id: Vec<u8>,
    charged_fee_e8s: u128,
    charged_gas_price_wei: u128,
}

#[derive(Clone, Debug)]
struct WrapQuote {
    charged_fee_e8s: u128,
    charged_gas_price_wei: u128,
    cycle_fee_e8s: u64,
    fee_ledger_canister: Principal,
}

#[derive(Clone, Debug)]
struct NormalizedDispatchUnwrapRequest {
    request_id: Vec<u8>,
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    recipient: Vec<u8>,
}

#[derive(Clone, Debug)]
struct NormalizedSubmitWrapRequest {
    request_id: RequestId,
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    evm_recipient: Vec<u8>,
    gas_limit: u64,
}

#[derive(Clone, Debug)]
struct NormalizedQuoteWrapRequest {
    gas_limit: u64,
}

#[derive(Clone, Debug)]
struct NormalizedUnwrapRequirementsArgs {
    asset_id: Vec<u8>,
    amount_e8s: Nat,
    caller_evm_address: Vec<u8>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
struct RequestId([u8; 32]);

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct QueueMeta {
    head: u64,
    tail: u64,
}

impl QueueMeta {
    fn new() -> Self {
        Self { head: 0, tail: 0 }
    }

    fn is_empty(&self) -> bool {
        self.head >= self.tail
    }
}

impl Storable for QueueMeta {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let mut out = [0u8; 16];
        out[..8].copy_from_slice(&self.head.to_be_bytes());
        out[8..].copy_from_slice(&self.tail.to_be_bytes());
        Cow::Owned(out.to_vec())
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let raw = bytes.as_ref();
        if raw.len() != 16 {
            return Self::new();
        }
        let mut head_raw = [0u8; 8];
        let mut tail_raw = [0u8; 8];
        head_raw.copy_from_slice(&raw[..8]);
        tail_raw.copy_from_slice(&raw[8..16]);
        Self {
            head: u64::from_be_bytes(head_raw),
            tail: u64::from_be_bytes(tail_raw),
        }
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 16,
        is_fixed_size: true,
    };
}

impl Storable for RequestId {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.0.to_vec())
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        let raw = bytes.as_ref();
        if raw.len() != 32 {
            return Self([0u8; 32]);
        }
        let mut out = [0u8; 32];
        out.copy_from_slice(raw);
        Self(out)
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: 32,
        is_fixed_size: true,
    };
}

impl Storable for StoredRequest {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let encoded =
            encode_stored_request(self).unwrap_or_else(|| panic!("stored_request.encode_failed"));
        Cow::Owned(encoded)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        decode_stored_request(bytes.as_ref())
            .unwrap_or_else(|| panic!("stored_request.decode_failed"))
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: STORED_REQUEST_MAX_BYTES,
        is_fixed_size: false,
    };
}

impl Storable for WrapStoredRequest {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let encoded = candid::encode_one(self)
            .unwrap_or_else(|_| panic!("wrap_stored_request.encode_failed"));
        Cow::Owned(encoded)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one::<WrapStoredRequest>(bytes.as_ref())
            .unwrap_or_else(|_| panic!("wrap_stored_request.decode_failed"))
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: WRAP_STORED_REQUEST_MAX_BYTES,
        is_fixed_size: false,
    };
}

impl Storable for FeePolicyStored {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let encoded =
            candid::encode_one(self).unwrap_or_else(|_| panic!("fee_policy.encode_failed"));
        Cow::Owned(encoded)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one::<FeePolicyStored>(bytes.as_ref())
            .unwrap_or_else(|_| panic!("fee_policy.decode_failed"))
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: FEE_POLICY_MAX_BYTES,
        is_fixed_size: false,
    };
}

impl Storable for WrapEvmConfigStored {
    fn to_bytes(&self) -> Cow<'_, [u8]> {
        let encoded =
            candid::encode_one(self).unwrap_or_else(|_| panic!("wrap_evm_config.encode_failed"));
        Cow::Owned(encoded)
    }

    fn from_bytes(bytes: Cow<'_, [u8]>) -> Self {
        candid::decode_one::<WrapEvmConfigStored>(bytes.as_ref())
            .unwrap_or_else(|_| panic!("wrap_evm_config.decode_failed"))
    }

    const BOUND: Bound = Bound::Bounded {
        max_size: WRAP_EVM_CONFIG_MAX_BYTES,
        is_fixed_size: false,
    };
}

struct StableState {
    kasane_canister: StableCell<Vec<u8>, Memory>,
    evm_gateway_canister: StableCell<Vec<u8>, Memory>,
    fee_policy: StableCell<FeePolicyStored, Memory>,
    wrap_evm_config: StableCell<WrapEvmConfigStored, Memory>,
    requests: StableBTreeMap<RequestId, StoredRequest, Memory>,
    queue: StableBTreeMap<u64, RequestId, Memory>,
    queue_meta: StableCell<QueueMeta, Memory>,
    wrap_requests: StableBTreeMap<RequestId, WrapStoredRequest, Memory>,
    wrap_queue: StableBTreeMap<u64, RequestId, Memory>,
    wrap_queue_meta: StableCell<QueueMeta, Memory>,
}

thread_local! {
    static MEMORY_MANAGER: RefCell<MemoryManager<DefaultMemoryImpl>> =
        RefCell::new(MemoryManager::init(DefaultMemoryImpl::default()));
    static STABLE_STATE: RefCell<Option<StableState>> = const { RefCell::new(None) };
    static WORKER_SCHEDULED: Cell<bool> = const { Cell::new(false) };
    static WRAP_WORKER_SCHEDULED: Cell<bool> = const { Cell::new(false) };
    static PENDING_WRAP_SUBMISSIONS: RefCell<BTreeSet<RequestId>> =
        const { RefCell::new(BTreeSet::new()) };
}

fn with_memory<R>(id: MemoryId, f: impl FnOnce(Memory) -> R) -> R {
    MEMORY_MANAGER.with(|m| {
        let memory = m.borrow().get(id);
        f(memory)
    })
}

fn init_state() {
    STABLE_STATE.with(|cell| {
        if cell.borrow().is_some() {
            return;
        }
        let kasane_canister = with_memory(MEM_KASANE_CANISTER, |memory| {
            StableCell::init(memory, Vec::<u8>::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell init failed: kasane_canister"))
        });
        let requests = with_memory(MEM_REQUESTS, StableBTreeMap::init);
        let queue = with_memory(MEM_QUEUE, StableBTreeMap::init);
        let queue_meta = with_memory(MEM_QUEUE_META, |memory| {
            StableCell::init(memory, QueueMeta::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell init failed: queue_meta"))
        });
        let evm_gateway_canister = with_memory(MEM_EVM_GATEWAY_CANISTER, |memory| {
            StableCell::init(memory, Vec::<u8>::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell init failed: evm_gateway_canister"))
        });
        let fee_policy = with_memory(MEM_FEE_POLICY, |memory| {
            StableCell::init(
                memory,
                FeePolicyStored {
                    fee_ledger_canister: Vec::new(),
                    cycle_fee_e8s: DEFAULT_CYCLE_FEE_E8S,
                    gas_price_buffer_bps: DEFAULT_GAS_PRICE_BUFFER_BPS,
                },
            )
            .unwrap_or_else(|_| ic_cdk::trap("stable cell init failed: fee_policy"))
        });
        let wrap_evm_config = with_memory(MEM_WRAP_EVM_CONFIG, |memory| {
            StableCell::init(
                memory,
                WrapEvmConfigStored {
                    wrap_factory_address: Vec::new(),
                },
            )
            .unwrap_or_else(|_| ic_cdk::trap("stable cell init failed: wrap_evm_config"))
        });
        let wrap_requests = with_memory(MEM_WRAP_REQUESTS, StableBTreeMap::init);
        let wrap_queue = with_memory(MEM_WRAP_QUEUE, StableBTreeMap::init);
        let wrap_queue_meta = with_memory(MEM_WRAP_QUEUE_META, |memory| {
            StableCell::init(memory, QueueMeta::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell init failed: wrap_queue_meta"))
        });
        *cell.borrow_mut() = Some(StableState {
            kasane_canister,
            evm_gateway_canister,
            fee_policy,
            wrap_evm_config,
            requests,
            queue,
            queue_meta,
            wrap_requests,
            wrap_queue,
            wrap_queue_meta,
        });
    });
}

fn with_state<R>(f: impl FnOnce(&StableState) -> R) -> R {
    STABLE_STATE.with(|cell| {
        let borrowed = cell.borrow();
        let state = borrowed
            .as_ref()
            .unwrap_or_else(|| ic_cdk::trap("stable_state: not initialized"));
        f(state)
    })
}

fn with_state_mut<R>(f: impl FnOnce(&mut StableState) -> R) -> R {
    STABLE_STATE.with(|cell| {
        let mut borrowed = cell.borrow_mut();
        let state = borrowed
            .as_mut()
            .unwrap_or_else(|| ic_cdk::trap("stable_state: not initialized"));
        f(state)
    })
}

#[ic_cdk::init]
fn init(args: InitArgs) {
    init_state();
    apply_runtime_config(args);
}

#[ic_cdk::post_upgrade]
fn post_upgrade(args: Option<InitArgs>) {
    init_state();
    let args = args.unwrap_or_else(|| {
        ic_cdk::trap(
            "UpgradeArgsRequired: InitArgs is required on upgrade; pass (opt record {...})",
        )
    });
    apply_runtime_config(args);
    let now = ic_cdk::api::time();
    if recover_request_state_after_upgrade(now) {
        schedule_worker();
    }
    if recover_wrap_request_state_after_upgrade(now) {
        schedule_wrap_worker();
    }
}

fn apply_runtime_config(args: InitArgs) {
    validate_runtime_config(&args).unwrap_or_else(|code| ic_cdk::trap(&code));
    with_state_mut(|state| {
        state
            .kasane_canister
            .set(args.kasane_canister.as_slice().to_vec())
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: kasane_canister"));
        state
            .evm_gateway_canister
            .set(args.evm_gateway_canister.as_slice().to_vec())
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: evm_gateway_canister"));
        state
            .fee_policy
            .set(FeePolicyStored {
                fee_ledger_canister: args.fee_ledger_canister.as_slice().to_vec(),
                cycle_fee_e8s: args.cycle_fee_e8s,
                gas_price_buffer_bps: args.gas_price_buffer_bps,
            })
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: fee_policy"));
        state
            .wrap_evm_config
            .set(WrapEvmConfigStored {
                wrap_factory_address: args.wrap_factory_address,
            })
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: wrap_evm_config"));
    });
}

fn validate_runtime_config(args: &InitArgs) -> Result<(), String> {
    validate_non_anonymous_principal(&args.kasane_canister, "arg.kasane_canister_anonymous")?;
    validate_non_anonymous_principal(
        &args.evm_gateway_canister,
        "arg.evm_gateway_canister_anonymous",
    )?;
    validate_non_anonymous_principal(
        &args.fee_ledger_canister,
        "arg.fee_ledger_canister_anonymous",
    )?;
    validate_gas_price_buffer_bps(args.gas_price_buffer_bps)?;
    validate_evm_address(
        args.wrap_factory_address.as_slice(),
        "arg.wrap_factory_address_invalid",
    )?;
    Ok(())
}

#[ic_cdk::update]
fn dispatch_unwrap_request(
    args: DispatchUnwrapRequestArgs,
) -> Result<DispatchUnwrapRequestOk, ApiError> {
    init_state();
    map_string_result(ensure_kasane_caller())?;
    let normalized = normalize_dispatch_unwrap_args(args).map_err(api_invalid_argument)?;
    let request_id = apply_insert_request_outcome(
        map_string_result(insert_request(normalized))?,
        schedule_worker,
    );
    Ok(DispatchUnwrapRequestOk {
        request_id: request_id.0.to_vec(),
    })
}

fn apply_insert_request_outcome(outcome: InsertRequestOutcome, scheduler: fn()) -> RequestId {
    match outcome {
        InsertRequestOutcome::Inserted(request_id) => {
            enqueue_request(request_id);
            scheduler();
            request_id
        }
        InsertRequestOutcome::AlreadyExists(request_id) => request_id,
    }
}

#[ic_cdk::query(composite = true)]
async fn quote_wrap_request(args: QuoteWrapRequestArgs) -> Result<QuoteWrapRequestOk, ApiError> {
    init_state();
    let normalized = normalize_quote_wrap_args(args).map_err(api_invalid_argument)?;
    let quote = quote_wrap_request_inner(normalized.gas_limit).await?;
    Ok(QuoteWrapRequestOk {
        charged_fee_e8s: Nat::from(quote.charged_fee_e8s),
        charged_gas_price_wei: Nat::from(quote.charged_gas_price_wei),
        cycle_fee_e8s: quote.cycle_fee_e8s,
        fee_ledger_canister: quote.fee_ledger_canister,
    })
}

#[ic_cdk::update]
async fn submit_wrap_request(args: SubmitWrapRequestArgs) -> Result<SubmitWrapRequestOk, ApiError> {
    init_state();
    let caller = ic_cdk::api::msg_caller();
    map_string_result(validate_non_anonymous_principal(
        &caller,
        "auth.caller_anonymous",
    ))?;
    let normalized = build_submit_wrap_request(args, caller).await?;
    let request_id = normalized.request_id;
    map_string_result(reserve_pending_wrap_submission(request_id))?;

    let out = submit_wrap_request_inner(normalized, caller, request_id).await;
    release_pending_wrap_submission(request_id);
    let (request_id, fee_charge) = out?;
    enqueue_wrap_request(request_id);
    schedule_wrap_worker();
    Ok(SubmitWrapRequestOk {
        request_id: request_id.0.to_vec(),
        charged_fee_e8s: Nat::from(fee_charge.charged_fee_e8s),
        charged_gas_price_wei: Nat::from(fee_charge.charged_gas_price_wei),
        fee_ledger_tx_id: fee_charge.ledger_tx_id,
    })
}

async fn submit_wrap_request_inner(
    args: NormalizedSubmitWrapRequest,
    caller: Principal,
    request_id: RequestId,
) -> Result<(RequestId, FeeCharge), ApiError> {
    let quote = quote_wrap_request_inner(args.gas_limit).await?;
    let fee_amount = u256_from_u128(quote.charged_fee_e8s);
    let fee_ledger_tx_id = attempt_icrc2_transfer_from(
        caller.as_slice().to_vec(),
        quote.fee_ledger_canister.as_slice().to_vec(),
        fee_amount.to_vec(),
    )
    .await
    .map_err(map_fee_collection_error)
    .map_err(api_rejected)?;
    let fee_charge = FeeCharge {
        ledger_tx_id: fee_ledger_tx_id,
        charged_fee_e8s: quote.charged_fee_e8s,
        charged_gas_price_wei: quote.charged_gas_price_wei,
    };
    let request_id = map_string_result(insert_wrap_request(
        args,
        caller,
        request_id,
        fee_charge.clone(),
    ))?;
    Ok((request_id, fee_charge))
}

#[ic_cdk::update]
async fn recover_failed_wrap(args: RecoverFailedWrapArgs) -> Result<RequestOverview, ApiError> {
    init_state();
    let request_id = request_id_or_invalid_argument(args.request_id.as_slice())?;
    let caller = ic_cdk::api::msg_caller();
    let (asset_id, amount) =
        map_string_result(reserve_failed_wrap_withdraw(request_id, caller))?;

    let transfer = attempt_icrc1_transfer(asset_id, amount, caller.as_slice().to_vec()).await;
    match transfer {
        Ok(tx_id) => {
            with_state_mut(|state| {
                if let Some(mut req) = state.wrap_requests.get(&request_id) {
                    req.result.withdrawn = true;
                    req.result.withdraw_ledger_tx_id = Some(tx_id.clone());
                    req.result.withdraw_error_code = None;
                    req.result.withdraw_in_progress = false;
                    req.result.mint_failed_recoverable = false;
                    state.wrap_requests.insert(request_id, req);
                }
            });
            request_overview_or_internal(request_id)
        }
        Err(code) => {
            let withdraw_code = to_withdraw_error_code(&code);
            with_state_mut(|state| {
                if let Some(mut req) = state.wrap_requests.get(&request_id) {
                    req.result.withdraw_error_code = Some(withdraw_code.clone());
                    req.result.withdraw_in_progress = false;
                    state.wrap_requests.insert(request_id, req);
                }
            });
            Err(api_rejected(withdraw_code))
        }
    }
}

#[ic_cdk::update]
async fn retry_request(args: RetryRequestArgs) -> Result<RequestOverview, ApiError> {
    init_state();
    let request_id = request_id_or_invalid_argument(args.request_id.as_slice())?;
    let caller = ic_cdk::api::msg_caller();
    map_string_result(validate_non_anonymous_principal(
        &caller,
        "auth.caller_anonymous",
    ))?;
    let (asset_id, amount, recipient) =
        map_string_result(reserve_failed_unwrap_retry(request_id, caller))?;
    let transfer = attempt_icrc1_transfer(asset_id, amount, recipient).await;
    with_state_mut(|state| {
        if let Some(mut req) = state.requests.get(&request_id) {
            apply_unwrap_transfer_result(&mut req, transfer);
            state.requests.insert(request_id, req);
        }
    });
    let status = with_state(|state| state.requests.get(&request_id).map(|req| req.result.status));
    if status != Some(RequestStatus::Succeeded) {
        return Err(api_rejected(with_state(|state| {
            state
                .requests
                .get(&request_id)
                .and_then(|req| req.result.error_code.clone())
                .unwrap_or_else(|| "unwrap.retry_failed".to_string())
        })));
    }
    request_overview_or_internal(request_id)
}

fn insert_request(args: NormalizedDispatchUnwrapRequest) -> Result<InsertRequestOutcome, String> {
    let request_id = to_request_id(args.request_id.as_slice())?;
    validate_principal_bytes(args.asset_id.as_slice())?;
    validate_principal_bytes(args.recipient.as_slice())?;
    with_state_mut(|state| {
        if let Some(existing) = state.requests.get(&request_id) {
            let same_payload = existing.asset_id == args.asset_id
                && existing.amount == args.amount
                && existing.recipient == args.recipient;
            return if same_payload {
                Ok(InsertRequestOutcome::AlreadyExists(request_id))
            } else {
                Err("request.idempotency_mismatch".to_string())
            };
        }
        state.requests.insert(
            request_id,
            StoredRequest {
                asset_id: args.asset_id,
                amount: args.amount,
                recipient: args.recipient,
                result: RequestResult {
                    status: RequestStatus::Queued,
                    ledger_tx_id: None,
                    error_code: None,
                    dispatch_status: Some(RequestDispatchStatusView::Dispatched),
                    dispatch_error: None,
                },
            },
        );
        Ok(InsertRequestOutcome::Inserted(request_id))
    })
}

fn reserve_failed_unwrap_retry(
    request_id: RequestId,
    caller: Principal,
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), String> {
    with_state_mut(|state| {
        let Some(mut req) = state.requests.get(&request_id) else {
            return Err("unwrap.retry_invalid_state".to_string());
        };
        let recipient = principal_from_bytes(req.recipient.as_slice())?;
        if recipient != caller {
            return Err("unwrap.retry_not_recipient".to_string());
        }
        if req.result.status == RequestStatus::Running {
            return Err("unwrap.retry_already_running".to_string());
        }
        if req.result.status != RequestStatus::Failed || req.result.ledger_tx_id.is_some() {
            return Err("unwrap.retry_invalid_state".to_string());
        }
        req.result.status = RequestStatus::Running;
        req.result.error_code = None;
        req.result.ledger_tx_id = None;
        let out = (
            req.asset_id.clone(),
            req.amount.clone(),
            req.recipient.clone(),
        );
        state.requests.insert(request_id, req);
        Ok(out)
    })
}

fn enqueue_request(request_id: RequestId) {
    with_state_mut(|state| {
        let mut meta = *state.queue_meta.get();
        let seq = meta.tail;
        meta.tail = meta.tail.saturating_add(1);
        state.queue.insert(seq, request_id);
        state
            .queue_meta
            .set(meta)
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: queue_meta"));
    });
}

fn dequeue_request() -> Option<RequestId> {
    with_state_mut(|state| {
        let mut meta = *state.queue_meta.get();
        let original_head = meta.head;
        let mut found = None;
        while meta.head < meta.tail {
            let seq = meta.head;
            meta.head = meta.head.saturating_add(1);
            if let Some(request_id) = state.queue.remove(&seq) {
                found = Some(request_id);
                break;
            }
        }
        if meta.head != original_head {
            state
                .queue_meta
                .set(meta)
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: queue_meta"));
        }
        found
    })
}

fn reserve_pending_wrap_submission(request_id: RequestId) -> Result<(), String> {
    let inserted = PENDING_WRAP_SUBMISSIONS.with(|pending| {
        let mut pending = pending.borrow_mut();
        if pending.contains(&request_id) {
            return false;
        }
        pending.insert(request_id)
    });
    if inserted {
        Ok(())
    } else {
        Err("wrap.request.pending".to_string())
    }
}

fn release_pending_wrap_submission(request_id: RequestId) {
    PENDING_WRAP_SUBMISSIONS.with(|pending| {
        pending.borrow_mut().remove(&request_id);
    });
}

fn insert_wrap_request(
    args: NormalizedSubmitWrapRequest,
    caller: Principal,
    request_id: RequestId,
    fee_charge: FeeCharge,
) -> Result<RequestId, String> {
    let inserted = with_state_mut(|state| {
        if state.wrap_requests.contains_key(&request_id) {
            return false;
        }
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: caller.as_slice().to_vec(),
                asset_id: args.asset_id,
                amount: args.amount,
                evm_recipient: args.evm_recipient,
                gas_limit: args.gas_limit,
                result: WrapRequestResult {
                    status: RequestStatus::Queued,
                    pull_ledger_tx_id: None,
                    mint_tx_id: None,
                    error_code: None,
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    withdraw_in_progress: false,
                    mint_failed_recoverable: false,
                    fee_ledger_tx_id: Some(fee_charge.ledger_tx_id),
                    charged_fee_e8s: Some(fee_charge.charged_fee_e8s),
                    charged_gas_price_wei: Some(fee_charge.charged_gas_price_wei),
                },
            },
        );
        true
    });
    if !inserted {
        return Err("wrap.request.duplicate".to_string());
    }
    Ok(request_id)
}

fn normalize_dispatch_unwrap_args(
    args: DispatchUnwrapRequestArgs,
) -> Result<NormalizedDispatchUnwrapRequest, String> {
    validate_principal_bytes(args.asset_id.as_slice())?;
    validate_principal_bytes(args.recipient.as_slice())?;
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s)
        .ok_or_else(|| "arg.amount_out_of_range".to_string())?
        .to_vec();
    validate_amount_bytes(amount.as_slice())?;
    Ok(NormalizedDispatchUnwrapRequest {
        request_id: args.request_id,
        asset_id: args.asset_id.as_slice().to_vec(),
        amount,
        recipient: args.recipient.as_slice().to_vec(),
    })
}

fn normalize_quote_wrap_args(
    args: QuoteWrapRequestArgs,
) -> Result<NormalizedQuoteWrapRequest, String> {
    validate_principal_bytes(args.asset_id.as_slice())?;
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s)
        .ok_or_else(|| "arg.amount_out_of_range".to_string())?;
    validate_amount_bytes(amount.as_slice())?;
    validate_evm_address(args.evm_recipient.as_slice(), "arg.evm_recipient_invalid")?;
    if args.gas_limit == 0 {
        return Err("arg.gas_limit_invalid".to_string());
    }
    Ok(NormalizedQuoteWrapRequest {
        gas_limit: args.gas_limit,
    })
}

fn normalize_submit_wrap_args(
    args: SubmitWrapRequestArgs,
) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>, u64, u64), String> {
    validate_principal_bytes(args.asset_id.as_slice())?;
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s)
        .ok_or_else(|| "arg.amount_out_of_range".to_string())?
        .to_vec();
    validate_amount_bytes(amount.as_slice())?;
    validate_evm_address(args.evm_recipient.as_slice(), "arg.evm_recipient_invalid")?;
    if args.gas_limit == 0 {
        return Err("arg.gas_limit_invalid".to_string());
    }
    Ok((
        args.asset_id.as_slice().to_vec(),
        amount,
        args.evm_recipient,
        args.evm_nonce,
        args.gas_limit,
    ))
}

fn normalize_unwrap_requirements_args(
    args: GetUnwrapRequirementsArgs,
) -> Result<NormalizedUnwrapRequirementsArgs, String> {
    validate_principal_bytes(args.asset_id.as_slice())?;
    validate_evm_address(
        args.caller_evm_address.as_slice(),
        "arg.caller_evm_address_invalid",
    )?;
    let amount_e8s = args.amount_e8s;
    let amount =
        nat_to_fixed_be::<32>(&amount_e8s).ok_or_else(|| "arg.amount_out_of_range".to_string())?;
    validate_amount_bytes(amount.as_slice())?;
    Ok(NormalizedUnwrapRequirementsArgs {
        asset_id: args.asset_id.as_slice().to_vec(),
        amount_e8s,
        caller_evm_address: args.caller_evm_address,
    })
}

fn api_invalid_argument(code: String) -> ApiError {
    ApiError::InvalidArgument(ApiErrorDetail {
        message: code.clone(),
        code,
    })
}

fn api_rejected(code: String) -> ApiError {
    ApiError::Rejected(ApiErrorDetail {
        message: code.clone(),
        code,
    })
}

fn api_internal(code: String) -> ApiError {
    ApiError::Internal(ApiErrorDetail {
        message: code.clone(),
        code,
    })
}

fn map_string_result<T>(result: Result<T, String>) -> Result<T, ApiError> {
    result.map_err(api_invalid_argument)
}

fn request_id_or_invalid_argument(bytes: &[u8]) -> Result<RequestId, ApiError> {
    to_request_id(bytes).map_err(api_invalid_argument)
}

fn request_error(code: Option<&str>) -> Option<RequestErrorView> {
    code.map(|value| RequestErrorView {
        code: value.to_string(),
        message: value.to_string(),
    })
}

fn wrap_request_overview(request_id: RequestId, req: &WrapStoredRequest) -> RequestOverview {
    RequestOverview {
        kind: RequestKind::Wrap,
        request_id: request_id.0.to_vec(),
        status: req.result.status,
        error: request_error(req.result.error_code.as_deref()),
        fee_ledger_tx_id: req.result.fee_ledger_tx_id.clone(),
        pull_ledger_tx_id: req.result.pull_ledger_tx_id.clone(),
        mint_tx_id: req.result.mint_tx_id.clone(),
        withdraw_ledger_tx_id: req.result.withdraw_ledger_tx_id.clone(),
        ledger_tx_id: None,
        dispatch_status: None,
        dispatch_error: None,
        charged_fee_e8s: req.result.charged_fee_e8s.map(Nat::from),
        charged_gas_price_wei: req.result.charged_gas_price_wei.map(Nat::from),
    }
}

fn unwrap_request_overview(request_id: RequestId, req: &StoredRequest) -> RequestOverview {
    RequestOverview {
        kind: RequestKind::Unwrap,
        request_id: request_id.0.to_vec(),
        status: req.result.status,
        error: request_error(req.result.error_code.as_deref()),
        fee_ledger_tx_id: None,
        pull_ledger_tx_id: None,
        mint_tx_id: None,
        withdraw_ledger_tx_id: None,
        ledger_tx_id: req.result.ledger_tx_id.clone(),
        dispatch_status: req.result.dispatch_status,
        dispatch_error: req.result.dispatch_error.clone(),
        charged_fee_e8s: None,
        charged_gas_price_wei: None,
    }
}

fn request_overview_or_internal(request_id: RequestId) -> Result<RequestOverview, ApiError> {
    get_request(request_id.0.to_vec()).ok_or_else(|| api_internal("request.not_found".to_string()))
}

async fn quote_wrap_request_inner(gas_limit: u64) -> Result<WrapQuote, ApiError> {
    let fee_policy = map_string_result(get_fee_policy_stored())?;
    let gas_price_wei = fetch_gas_price_wei_from_gateway()
        .await
        .map_err(api_rejected)?;
    let charged_gas_price_wei = ceil_mul_ratio_u128(
        gas_price_wei,
        u128::from(fee_policy.gas_price_buffer_bps),
        GAS_PRICE_DENOMINATOR_BPS,
    );
    let charged_fee_e8s =
        compute_total_fee_e8s(gas_limit, charged_gas_price_wei, fee_policy.cycle_fee_e8s)
            .map_err(api_rejected)?;
    Ok(WrapQuote {
        charged_fee_e8s,
        charged_gas_price_wei,
        cycle_fee_e8s: fee_policy.cycle_fee_e8s,
        fee_ledger_canister: principal_from_bytes(fee_policy.fee_ledger_canister.as_slice())
            .map_err(api_internal)?,
    })
}

fn selector(signature: &[u8]) -> [u8; 4] {
    let mut keccak = Keccak::v256();
    keccak.update(signature);
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    [out[0], out[1], out[2], out[3]]
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
    let padded_len = ((asset_id.len() + 31) / 32) * 32;
    let mut out = Vec::with_capacity(4 + 32 * 3 + padded_len);
    out.extend_from_slice(&selector(b"getTokenAddress(bytes)"));
    out.extend_from_slice(&u256_from_u128(32));
    out.extend_from_slice(&u256_from_u128(asset_id.len() as u128));
    out.extend_from_slice(asset_id);
    out.resize(4 + 32 * 2 + padded_len, 0);
    out
}

async fn with_expected_wrap_nonce_from_gateway() -> Result<u64, String> {
    let gateway = expected_evm_gateway_canister()?;
    let wrap_evm =
        ic_evm_address::derive_evm_address_from_principal(ic_cdk::api::canister_self().as_slice())
            .map_err(|_| "wrap.evm_address_derivation_failed".to_string())?;
    let call_result = ic_cdk::call::Call::unbounded_wait(gateway, "expected_nonce_by_address")
        .with_arg(wrap_evm.to_vec())
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<u64, String>,)>() {
            Ok((Ok(nonce),)) => Ok(nonce),
            Ok((Err(code),)) => Err(format!("wrap.nonce_failed:{code}")),
            Err(err) => Err(format!("wrap.nonce_decode_failed:{err}")),
        },
        Err(err) => Err(format!("wrap.nonce_call_failed:{err}")),
    }
}

async fn build_submit_wrap_request(
    args: SubmitWrapRequestArgs,
    caller: Principal,
) -> Result<NormalizedSubmitWrapRequest, ApiError> {
    let (asset_id, amount, evm_recipient, evm_nonce, gas_limit) =
        normalize_submit_wrap_args(args).map_err(api_invalid_argument)?;
    let request_id = RequestId(derive_wrap_request_id(
        caller.as_slice(),
        asset_id.as_slice(),
        amount.as_slice(),
        evm_recipient.as_slice(),
        evm_nonce,
        gas_limit,
    ));
    if with_state(|state| state.wrap_requests.contains_key(&request_id)) {
        return Err(api_invalid_argument("wrap.request.duplicate".to_string()));
    }
    Ok(NormalizedSubmitWrapRequest {
        request_id,
        asset_id,
        amount,
        evm_recipient,
        gas_limit,
    })
}

async fn gateway_rpc_eth_call(call: RpcCallObjectView) -> Result<RpcCallResultView, ApiError> {
    let gateway = expected_evm_gateway_canister().map_err(api_internal)?;
    let call_result = ic_cdk::call::Call::unbounded_wait(gateway, "rpc_eth_call_object")
        .with_arg(call)
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<RpcCallResultView, RpcErrorView>,)>() {
            Ok((Ok(value),)) => Ok(value),
            Ok((Err(err),)) => Err(api_rejected(format!(
                "rpc.call_failed:{}:{}",
                err.code, err.message
            ))),
            Err(err) => Err(api_internal(format!("rpc.call_decode_failed:{err}"))),
        },
        Err(err) => Err(api_rejected(format!("rpc.call_transport_failed:{err}"))),
    }
}

fn decode_u256_be(bytes: &[u8]) -> Result<[u8; 32], ApiError> {
    if bytes.len() < 32 {
        return Err(api_internal("rpc.return_data_short".to_string()));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes[..32]);
    Ok(out)
}

fn zero_eth_value_word() -> Vec<u8> {
    vec![0u8; 32]
}

async fn fetch_wrapped_token_address(
    asset_id: &[u8],
    caller_evm_address: &[u8],
) -> Result<Option<Vec<u8>>, ApiError> {
    let factory = expected_evm_wrap_factory().map_err(api_internal)?;
    let result = gateway_rpc_eth_call(RpcCallObjectView {
        to: Some(factory),
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
    .await?;
    if result.return_data.len() < 32 {
        return Ok(None);
    }
    let address = result.return_data[result.return_data.len() - 20..].to_vec();
    if address.iter().all(|byte| *byte == 0) {
        return Ok(None);
    }
    Ok(Some(address))
}

async fn fetch_erc20_balance(token: &[u8], owner: &[u8]) -> Result<Nat, ApiError> {
    let result = gateway_rpc_eth_call(RpcCallObjectView {
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
    .await?;
    Ok(Nat(BigUint::from_bytes_be(&decode_u256_be(
        result.return_data.as_slice(),
    )?)))
}

async fn fetch_erc20_allowance(
    token: &[u8],
    owner: &[u8],
    spender: &[u8],
) -> Result<Nat, ApiError> {
    let result = gateway_rpc_eth_call(RpcCallObjectView {
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
    .await?;
    Ok(Nat(BigUint::from_bytes_be(&decode_u256_be(
        result.return_data.as_slice(),
    )?)))
}

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

fn approval_required_for_readiness(readiness: UnwrapReadiness) -> bool {
    readiness == UnwrapReadiness::InsufficientAllowance
}

fn hash_len_prefixed(hasher: &mut Keccak, bytes: &[u8]) {
    let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
    hasher.update(&len.to_be_bytes());
    hasher.update(bytes);
}

fn enqueue_wrap_request(request_id: RequestId) {
    with_state_mut(|state| {
        let mut meta = *state.wrap_queue_meta.get();
        let seq = meta.tail;
        meta.tail = meta.tail.saturating_add(1);
        state.wrap_queue.insert(seq, request_id);
        state
            .wrap_queue_meta
            .set(meta)
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: wrap_queue_meta"));
    });
}

fn dequeue_wrap_request() -> Option<RequestId> {
    with_state_mut(|state| {
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
            state
                .wrap_queue_meta
                .set(meta)
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: wrap_queue_meta"));
        }
        found
    })
}

#[ic_cdk::query]
fn get_request(request_id: Vec<u8>) -> Option<RequestOverview> {
    init_state();
    let request_id = to_request_id(request_id.as_slice()).ok()?;
    with_state(|state| {
        state
            .wrap_requests
            .get(&request_id)
            .map(|v| wrap_request_overview(request_id, &v))
            .or_else(|| {
                state
                    .requests
                    .get(&request_id)
                    .map(|v| unwrap_request_overview(request_id, &v))
            })
    })
}

#[ic_cdk::query(composite = true)]
async fn get_unwrap_requirements(
    args: GetUnwrapRequirementsArgs,
) -> Result<GetUnwrapRequirementsOk, ApiError> {
    init_state();
    let normalized = normalize_unwrap_requirements_args(args).map_err(api_invalid_argument)?;
    let factory_address = expected_evm_wrap_factory().map_err(api_internal)?;
    let token_address = fetch_wrapped_token_address(
        normalized.asset_id.as_slice(),
        normalized.caller_evm_address.as_slice(),
    )
    .await?;
    if token_address.is_none() {
        return Ok(GetUnwrapRequirementsOk {
            factory_address,
            wrapped_token_address: None,
            balance: Nat::from(0u8),
            allowance: Nat::from(0u8),
            approve_required: approval_required_for_readiness(UnwrapReadiness::TokenNotDeployed),
            readiness: UnwrapReadiness::TokenNotDeployed,
        });
    }
    let token_address = token_address.expect("checked");
    let balance = fetch_erc20_balance(
        token_address.as_slice(),
        normalized.caller_evm_address.as_slice(),
    )
    .await?;
    let allowance = fetch_erc20_allowance(
        token_address.as_slice(),
        normalized.caller_evm_address.as_slice(),
        factory_address.as_slice(),
    )
    .await?;
    let amount = normalized.amount_e8s;
    let readiness = if balance < amount {
        UnwrapReadiness::InsufficientBalance
    } else if allowance < amount {
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

#[ic_cdk::query]
fn get_fee_policy() -> Result<FeePolicyView, String> {
    init_state();
    let stored = get_fee_policy_stored()?;
    Ok(FeePolicyView {
        fee_ledger_canister: principal_from_bytes(stored.fee_ledger_canister.as_slice())?,
        cycle_fee_e8s: stored.cycle_fee_e8s,
        gas_price_buffer_bps: stored.gas_price_buffer_bps,
    })
}

fn recover_request_state_after_upgrade(_now: u64) -> bool {
    with_state_mut(|state| {
        let mut queued_ids = BTreeSet::new();
        for (_, request_id) in state.queue.range(..) {
            queued_ids.insert(request_id);
        }

        let mut candidates = Vec::new();
        for (request_id, stored) in state.requests.range(..) {
            let mut req = stored.clone();
            match req.result.status {
                RequestStatus::Queued => {
                    if !queued_ids.contains(&request_id) {
                        candidates.push((request_id, req));
                    }
                }
                RequestStatus::Running => {
                    req.result.status = RequestStatus::Queued;
                    candidates.push((request_id, req));
                }
                RequestStatus::Succeeded | RequestStatus::Failed => {}
            }
        }

        if candidates.is_empty() {
            return !state.queue_meta.get().is_empty();
        }

        let mut meta = *state.queue_meta.get();
        for (request_id, req) in candidates {
            state.requests.insert(request_id, req);
            if queued_ids.contains(&request_id) {
                continue;
            }
            let seq = meta.tail;
            meta.tail = meta.tail.saturating_add(1);
            state.queue.insert(seq, request_id);
            queued_ids.insert(request_id);
        }
        state
            .queue_meta
            .set(meta)
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: queue_meta"));
        !state.queue_meta.get().is_empty()
    })
}

fn recover_wrap_request_state_after_upgrade(_now: u64) -> bool {
    with_state_mut(|state| {
        let mut queued_ids = BTreeSet::new();
        for (_, request_id) in state.wrap_queue.range(..) {
            queued_ids.insert(request_id);
        }

        let mut in_progress_to_clear = Vec::new();
        let mut candidates = Vec::new();
        for (request_id, stored) in state.wrap_requests.range(..) {
            let mut req = stored.clone();
            if req.result.withdraw_in_progress {
                req.result.withdraw_in_progress = false;
                in_progress_to_clear.push((request_id, req.clone()));
            }
            match req.result.status {
                RequestStatus::Queued => {
                    if !queued_ids.contains(&request_id) {
                        candidates.push((request_id, req));
                    }
                }
                RequestStatus::Running => {
                    req.result.status = RequestStatus::Queued;
                    candidates.push((request_id, req));
                }
                RequestStatus::Succeeded | RequestStatus::Failed => {}
            }
        }

        for (request_id, req) in in_progress_to_clear {
            state.wrap_requests.insert(request_id, req);
        }

        if candidates.is_empty() {
            return !state.wrap_queue_meta.get().is_empty();
        }

        let mut meta = *state.wrap_queue_meta.get();
        for (request_id, req) in candidates {
            state.wrap_requests.insert(request_id, req);
            if queued_ids.contains(&request_id) {
                continue;
            }
            let seq = meta.tail;
            meta.tail = meta.tail.saturating_add(1);
            state.wrap_queue.insert(seq, request_id);
            queued_ids.insert(request_id);
        }
        state
            .wrap_queue_meta
            .set(meta)
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: wrap_queue_meta"));
        !state.wrap_queue_meta.get().is_empty()
    })
}

#[ic_cdk::update]
fn set_fee_policy(args: SetFeePolicyArgs) -> Result<(), String> {
    init_state();
    ensure_controller_caller()?;
    validate_non_anonymous_principal(
        &args.fee_ledger_canister,
        "arg.fee_ledger_canister_anonymous",
    )?;
    validate_gas_price_buffer_bps(args.gas_price_buffer_bps)?;
    with_state_mut(|state| {
        state
            .fee_policy
            .set(FeePolicyStored {
                fee_ledger_canister: args.fee_ledger_canister.as_slice().to_vec(),
                cycle_fee_e8s: args.cycle_fee_e8s,
                gas_price_buffer_bps: args.gas_price_buffer_bps,
            })
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: fee_policy"));
    });
    Ok(())
}

fn ensure_kasane_caller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    let expected = with_state(|state| state.kasane_canister.get().clone());
    if expected.is_empty() {
        return Err("config.kasane_missing".to_string());
    }
    let principal = principal_from_bytes(expected.as_slice())?;
    if principal == caller {
        Ok(())
    } else {
        Err("auth.kasane_required".to_string())
    }
}

fn ensure_controller_caller() -> Result<(), String> {
    let caller = ic_cdk::api::msg_caller();
    if ic_cdk::api::is_controller(&caller) {
        Ok(())
    } else {
        Err("auth.controller_required".to_string())
    }
}

fn validate_withdraw_request(req: &WrapStoredRequest, caller: Principal) -> Result<(), String> {
    if req.caller.as_slice() != caller.as_slice() {
        return Err("withdraw.not_request_owner".to_string());
    }
    if req.result.withdrawn {
        return Err("withdraw.already_withdrawn".to_string());
    }
    if req.result.withdraw_in_progress {
        return Err("withdraw.in_progress".to_string());
    }
    if !is_withdrawable(req) {
        return Err("withdraw.invalid_state".to_string());
    }
    Ok(())
}

fn is_withdrawable(req: &WrapStoredRequest) -> bool {
    req.result.status == RequestStatus::Failed
        && req.result.mint_failed_recoverable
        && !req.result.withdrawn
        && req.result.pull_ledger_tx_id.is_some()
        && req.result.mint_tx_id.is_none()
}

fn to_withdraw_error_code(code: &str) -> String {
    if let Some(suffix) = code.strip_prefix("ledger.transfer_failed:") {
        return format!("withdraw.transfer_failed:{suffix}");
    }
    if let Some(suffix) = code.strip_prefix("ledger.decode_failed:") {
        return format!("withdraw.decode_failed:{suffix}");
    }
    if let Some(suffix) = code.strip_prefix("ledger.call_failed:") {
        return format!("withdraw.call_failed:{suffix}");
    }
    "withdraw.invalid_state".to_string()
}

fn reserve_failed_wrap_withdraw(
    request_id: RequestId,
    caller: Principal,
) -> Result<(Vec<u8>, Vec<u8>), String> {
    with_state_mut(|state| {
        let Some(mut req) = state.wrap_requests.get(&request_id) else {
            return Err("withdraw.invalid_state".to_string());
        };
        validate_withdraw_request(&req, caller)?;
        req.result.withdraw_in_progress = true;
        req.result.withdraw_error_code = None;
        let out = (req.asset_id.clone(), req.amount.clone());
        state.wrap_requests.insert(request_id, req);
        Ok(out)
    })
}

fn schedule_worker() {
    let should_schedule = WORKER_SCHEDULED.with(|f| {
        if f.get() {
            return false;
        }
        f.set(true);
        true
    });
    if !should_schedule {
        return;
    }
    ic_cdk_timers::set_timer(std::time::Duration::from_millis(50), async move {
        worker_tick().await;
    });
}

fn schedule_wrap_worker() {
    let should_schedule = WRAP_WORKER_SCHEDULED.with(|f| {
        if f.get() {
            return false;
        }
        f.set(true);
        true
    });
    if !should_schedule {
        return;
    }
    ic_cdk_timers::set_timer(std::time::Duration::from_millis(50), async move {
        wrap_worker_tick().await;
    });
}

async fn worker_tick() {
    loop {
        let Some(request_id) = dequeue_request() else {
            on_worker_queue_drain();
            return;
        };
        let req = with_state(|state| {
            state.requests.get(&request_id).map(|req| {
                (
                    req.asset_id.clone(),
                    req.amount.clone(),
                    req.recipient.clone(),
                )
            })
        });
        let Some((asset_id, amount, recipient)) = req else {
            continue;
        };
        mark_request_running(request_id);
        let result = attempt_icrc1_transfer(asset_id, amount, recipient).await;
        with_state_mut(|state| {
            if let Some(mut req) = state.requests.get(&request_id) {
                apply_unwrap_transfer_result(&mut req, result);
                state.requests.insert(request_id, req);
            }
        });
    }
}

fn apply_unwrap_transfer_result(req: &mut StoredRequest, result: Result<Vec<u8>, String>) {
    match result {
        Ok(tx_id) => {
            req.result.status = RequestStatus::Succeeded;
            req.result.ledger_tx_id = Some(tx_id);
            req.result.error_code = None;
            req.result.dispatch_status = Some(RequestDispatchStatusView::Dispatched);
            req.result.dispatch_error = None;
        }
        Err(code) => {
            req.result.status = RequestStatus::Failed;
            req.result.ledger_tx_id = None;
            req.result.error_code = Some(code);
            req.result.dispatch_status = Some(RequestDispatchStatusView::Dispatched);
            req.result.dispatch_error = None;
        }
    }
}

async fn wrap_worker_tick() {
    loop {
        let Some(request_id) = dequeue_wrap_request() else {
            on_wrap_worker_queue_drain();
            return;
        };
        let req = with_state(|state| {
            state.wrap_requests.get(&request_id).map(|req| {
                (
                    req.caller.clone(),
                    req.asset_id.clone(),
                    req.amount.clone(),
                    req.evm_recipient.clone(),
                    req.gas_limit,
                    req.result.charged_gas_price_wei.unwrap_or(0),
                )
            })
        });
        let Some((caller, asset_id, amount, evm_recipient, gas_limit, charged_gas_price_wei)) = req
        else {
            continue;
        };
        mark_wrap_request_running(request_id);

        let pull = attempt_icrc2_transfer_from(caller, asset_id.clone(), amount.clone()).await;
        let outcome = match pull {
            Ok(pull_tx_id) => match with_expected_wrap_nonce_from_gateway().await {
                Ok(evm_nonce) => match fetch_max_priority_fee_wei_from_gateway().await {
                    Ok(suggested_priority_fee_wei) => {
                        let mint = submit_mint_tx(
                            asset_id,
                            evm_recipient,
                            amount,
                            evm_nonce,
                            gas_limit,
                            charged_gas_price_wei,
                            suggested_priority_fee_wei,
                        )
                        .await;
                        match mint {
                            Ok(mint_tx_id) => WrapExecutionOutcome {
                                status: RequestStatus::Succeeded,
                                pull_ledger_tx_id: Some(pull_tx_id),
                                mint_tx_id: Some(mint_tx_id),
                                error_code: None,
                                mint_failed_recoverable: false,
                            },
                            Err(code) => WrapExecutionOutcome {
                                status: RequestStatus::Failed,
                                pull_ledger_tx_id: Some(pull_tx_id),
                                mint_tx_id: None,
                                error_code: Some(code),
                                mint_failed_recoverable: true,
                            },
                        }
                    }
                    Err(code) => WrapExecutionOutcome {
                        status: RequestStatus::Failed,
                        pull_ledger_tx_id: Some(pull_tx_id),
                        mint_tx_id: None,
                        error_code: Some(code),
                        mint_failed_recoverable: true,
                    },
                },
                Err(code) => WrapExecutionOutcome {
                    status: RequestStatus::Failed,
                    pull_ledger_tx_id: Some(pull_tx_id),
                    mint_tx_id: None,
                    error_code: Some(format!(
                        "evm_gateway.submit_failed:rejected:nonce_allocation_failed:{code}"
                    )),
                    mint_failed_recoverable: true,
                },
            },
            Err(code) => WrapExecutionOutcome {
                status: RequestStatus::Failed,
                pull_ledger_tx_id: None,
                mint_tx_id: None,
                error_code: Some(code),
                mint_failed_recoverable: false,
            },
        };

        with_state_mut(|state| {
            if let Some(mut req) = state.wrap_requests.get(&request_id) {
                req.result.status = outcome.status;
                req.result.pull_ledger_tx_id = outcome.pull_ledger_tx_id;
                req.result.mint_tx_id = outcome.mint_tx_id;
                req.result.error_code = outcome.error_code;
                req.result.mint_failed_recoverable = outcome.mint_failed_recoverable;
                state.wrap_requests.insert(request_id, req);
            }
        });
    }
}

fn on_worker_queue_drain() {
    WORKER_SCHEDULED.with(|f| f.set(false));
    if with_state(|state| !state.queue_meta.get().is_empty()) {
        schedule_worker();
    }
}

fn on_wrap_worker_queue_drain() {
    WRAP_WORKER_SCHEDULED.with(|f| f.set(false));
    if with_state(|state| !state.wrap_queue_meta.get().is_empty()) {
        schedule_wrap_worker();
    }
}

struct WrapExecutionOutcome {
    status: RequestStatus,
    pull_ledger_tx_id: Option<Vec<u8>>,
    mint_tx_id: Option<Vec<u8>>,
    error_code: Option<String>,
    mint_failed_recoverable: bool,
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

#[derive(Clone, Debug, CandidType, Deserialize)]
enum Icrc1MetadataValue {
    Int(candid::Int),
    Nat(Nat),
    Blob(Vec<u8>),
    Text(String),
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct SubmitIcTxArgsDto {
    to: Option<Vec<u8>>,
    from: Option<Vec<u8>>,
    value: Nat,
    gas_limit: u64,
    nonce: u64,
    max_fee_per_gas: Nat,
    max_priority_fee_per_gas: Nat,
    data: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
enum SubmitTxError {
    InvalidArgument(String),
    Rejected(String),
    Internal(String),
}

async fn attempt_icrc1_transfer(
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    recipient: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let ledger = principal_from_bytes(asset_id.as_slice())?;
    let to = principal_from_bytes(recipient.as_slice())?;
    let nat_amount = nat_from_32_be(amount.as_slice())?;
    let arg = Icrc1TransferArg {
        from_subaccount: None,
        to: Icrc1Account {
            owner: to,
            subaccount: None,
        },
        amount: nat_amount,
        fee: None,
        memo: None,
        created_at_time: Some(ic_cdk::api::time()),
    };
    let call_result = ic_cdk::call::Call::unbounded_wait(ledger, "icrc1_transfer")
        .with_arg(arg)
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<Nat, Icrc1TransferError>,)>() {
            Ok((result,)) => map_transfer_reply(result),
            Err(err) => Err(format!("ledger.decode_failed:{err}")),
        },
        Err(err) => Err(format!("ledger.call_failed:{err}")),
    }
}

async fn attempt_icrc2_transfer_from(
    caller: Vec<u8>,
    asset_id: Vec<u8>,
    amount: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let ledger = principal_from_bytes(asset_id.as_slice())?;
    let caller_principal = principal_from_bytes(caller.as_slice())?;
    let to = ic_cdk::api::canister_self();
    let nat_amount = nat_from_32_be(amount.as_slice())?;
    let arg = Icrc2TransferFromArg {
        from: Icrc1Account {
            owner: caller_principal,
            subaccount: None,
        },
        spender_subaccount: None,
        to: Icrc1Account {
            owner: to,
            subaccount: None,
        },
        amount: nat_amount,
        fee: None,
        memo: None,
        created_at_time: Some(ic_cdk::api::time()),
    };
    let call_result = ic_cdk::call::Call::unbounded_wait(ledger, "icrc2_transfer_from")
        .with_arg(arg)
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<Nat, Icrc2TransferFromError>,)>() {
            Ok((result,)) => match result {
                Ok(block_index) => Ok(nat_to_be_bytes(&block_index)),
                Err(err) => Err(format!(
                    "ledger.transfer_from_failed:{}",
                    transfer_from_error_to_code(&err)
                )),
            },
            Err(err) => Err(format!("ledger.decode_failed:{err}")),
        },
        Err(err) => Err(format!("ledger.call_failed:{err}")),
    }
}

async fn fetch_asset_decimals(asset_id: &[u8]) -> Result<u8, String> {
    validate_principal_bytes(asset_id)?;
    let ledger = principal_from_bytes(asset_id)?;
    let call_result = ic_cdk::call::Call::unbounded_wait(ledger, "icrc1_metadata").await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Vec<(String, Icrc1MetadataValue)>,)>() {
            Ok((metadata,)) => decode_asset_decimals(metadata.as_slice()),
            Err(err) => Err(format!("wrap.asset_metadata_failed:decode_failed:{err}")),
        },
        Err(err) => Err(format!("wrap.asset_metadata_failed:call_failed:{err}")),
    }
}

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

async fn submit_mint_tx(
    asset_id: Vec<u8>,
    evm_recipient: Vec<u8>,
    amount: Vec<u8>,
    nonce: u64,
    gas_limit: u64,
    charged_gas_price_wei: u128,
    suggested_priority_fee_wei: u128,
) -> Result<Vec<u8>, String> {
    let gateway = expected_evm_gateway_canister()?;
    let args = build_submit_mint_tx_args(
        asset_id,
        evm_recipient,
        amount,
        nonce,
        gas_limit,
        charged_gas_price_wei,
        suggested_priority_fee_wei,
    )
    .await?;

    let call_result = ic_cdk::call::Call::unbounded_wait(gateway, "submit_ic_tx")
        .with_arg(args)
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<Vec<u8>, SubmitTxError>,)>() {
            Ok((result,)) => match result {
                Ok(tx_id) => Ok(tx_id),
                Err(err) => Err(format!(
                    "evm_gateway.submit_failed:{}",
                    submit_error_to_code(err)
                )),
            },
            Err(err) => Err(format!("evm_gateway.decode_failed:{err}")),
        },
        Err(err) => Err(format!("evm_gateway.call_failed:{err}")),
    }
}

async fn fetch_gas_price_wei_from_gateway() -> Result<u128, String> {
    let gateway = expected_evm_gateway_canister()?;
    let call_result = ic_cdk::call::Call::unbounded_wait(gateway, "rpc_eth_gas_price").await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<Nat, RpcErrorView>,)>() {
            Ok((result,)) => match result {
                Ok(value) => {
                    nat_to_u128(&value).ok_or_else(|| "fee.quote_out_of_range".to_string())
                }
                Err(err) => Err(format!("fee.quote_failed:{}:{}", err.code, err.message)),
            },
            Err(err) => Err(format!("fee.quote_decode_failed:{err}")),
        },
        Err(err) => Err(format!("fee.quote_call_failed:{err}")),
    }
}

async fn fetch_max_priority_fee_wei_from_gateway() -> Result<u128, String> {
    let gateway = expected_evm_gateway_canister()?;
    let call_result =
        ic_cdk::call::Call::unbounded_wait(gateway, "rpc_eth_max_priority_fee_per_gas").await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<Nat, RpcErrorView>,)>() {
            Ok((result,)) => match result {
                Ok(value) => {
                    nat_to_u128(&value).ok_or_else(|| "fee.priority_out_of_range".to_string())
                }
                Err(err) => Err(format!("fee.priority_failed:{}:{}", err.code, err.message)),
            },
            Err(err) => Err(format!("fee.priority_decode_failed:{err}")),
        },
        Err(err) => Err(format!("fee.priority_call_failed:{err}")),
    }
}

async fn build_submit_mint_tx_args(
    asset_id: Vec<u8>,
    evm_recipient: Vec<u8>,
    amount: Vec<u8>,
    nonce: u64,
    gas_limit: u64,
    charged_gas_price_wei: u128,
    suggested_priority_fee_wei: u128,
) -> Result<SubmitIcTxArgsDto, String> {
    validate_principal_bytes(asset_id.as_slice())?;
    validate_evm_address(evm_recipient.as_slice(), "arg.evm_recipient_invalid")?;
    validate_amount_bytes(amount.as_slice())?;
    let factory = expected_evm_wrap_factory()?;
    let token_decimals = fetch_asset_decimals(asset_id.as_slice()).await?;
    let data = encode_factory_mint_for_asset_call_data(
        asset_id.as_slice(),
        token_decimals,
        evm_recipient.as_slice(),
        amount.as_slice(),
    )?;
    Ok(build_submit_ic_tx_args(
        factory,
        nonce,
        gas_limit,
        charged_gas_price_wei,
        suggested_priority_fee_wei,
        data,
    ))
}

fn build_submit_ic_tx_args(
    factory: Vec<u8>,
    nonce: u64,
    gas_limit: u64,
    charged_gas_price_wei: u128,
    suggested_priority_fee_wei: u128,
    data: Vec<u8>,
) -> SubmitIcTxArgsDto {
    let capped_priority_fee_wei = suggested_priority_fee_wei.min(charged_gas_price_wei);
    SubmitIcTxArgsDto {
        to: Some(factory),
        from: None,
        value: Nat::from(0u8),
        gas_limit,
        nonce,
        max_fee_per_gas: Nat::from(charged_gas_price_wei),
        max_priority_fee_per_gas: Nat::from(capped_priority_fee_wei),
        data,
    }
}

fn map_transfer_reply(result: Result<Nat, Icrc1TransferError>) -> Result<Vec<u8>, String> {
    match result {
        Ok(block_index) => Ok(nat_to_be_bytes(&block_index)),
        Err(err) => Err(format!(
            "ledger.transfer_failed:{}",
            transfer_error_to_code(&err)
        )),
    }
}

fn transfer_error_to_code(error: &Icrc1TransferError) -> String {
    match error {
        Icrc1TransferError::BadFee { expected_fee } => {
            format!("bad_fee:{}", expected_fee.0)
        }
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
        } => {
            format!("generic_error:{}:{message}", error_code.0)
        }
    }
}

fn transfer_from_error_to_code(error: &Icrc2TransferFromError) -> String {
    match error {
        Icrc2TransferFromError::BadFee { expected_fee } => {
            format!("bad_fee:{}", expected_fee.0)
        }
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
        } => {
            format!("generic_error:{}:{message}", error_code.0)
        }
    }
}

fn submit_error_to_code(error: SubmitTxError) -> String {
    match error {
        SubmitTxError::InvalidArgument(code) => format!("invalid_argument:{code}"),
        SubmitTxError::Rejected(code) => format!("rejected:{code}"),
        SubmitTxError::Internal(code) => format!("internal:{code}"),
    }
}

fn nat_to_be_bytes(value: &Nat) -> Vec<u8> {
    let bytes = value.0.to_bytes_be();
    if bytes.is_empty() {
        return vec![0u8];
    }
    bytes
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

fn mark_request_running(request_id: RequestId) {
    with_state_mut(|state| {
        if let Some(mut req) = state.requests.get(&request_id) {
            req.result.status = RequestStatus::Running;
            req.result.ledger_tx_id = None;
            req.result.error_code = None;
            req.result.dispatch_status = Some(RequestDispatchStatusView::Dispatched);
            req.result.dispatch_error = None;
            state.requests.insert(request_id, req);
        }
    });
}

fn mark_wrap_request_running(request_id: RequestId) {
    with_state_mut(|state| {
        if let Some(mut req) = state.wrap_requests.get(&request_id) {
            req.result.status = RequestStatus::Running;
            req.result.pull_ledger_tx_id = None;
            req.result.mint_tx_id = None;
            req.result.error_code = None;
            req.result.withdrawn = false;
            req.result.withdraw_ledger_tx_id = None;
            req.result.withdraw_error_code = None;
            req.result.withdraw_in_progress = false;
            req.result.mint_failed_recoverable = false;
            state.wrap_requests.insert(request_id, req);
        }
    });
}

fn nat_from_32_be(amount: &[u8]) -> Result<Nat, String> {
    if amount.len() != 32 {
        return Err("arg.amount_invalid".to_string());
    }
    Ok(Nat(BigUint::from_bytes_be(amount)))
}

fn validate_amount_bytes(amount: &[u8]) -> Result<(), String> {
    if amount.len() != AMOUNT_BYTES {
        return Err("arg.amount_invalid".to_string());
    }
    Ok(())
}

fn validate_evm_address(bytes: &[u8], code: &str) -> Result<(), String> {
    if bytes.len() != EVM_ADDRESS_BYTES {
        return Err(code.to_string());
    }
    Ok(())
}

fn validate_principal_bytes(bytes: &[u8]) -> Result<(), String> {
    if !(1..=PRINCIPAL_MAX_BYTES).contains(&bytes.len()) {
        return Err("arg.principal_invalid".to_string());
    }
    Ok(())
}

fn validate_non_anonymous_principal(principal: &Principal, code: &str) -> Result<(), String> {
    if *principal == Principal::anonymous() {
        return Err(code.to_string());
    }
    Ok(())
}

fn validate_gas_price_buffer_bps(value: u32) -> Result<(), String> {
    if !(10_000..=50_000).contains(&value) {
        return Err("arg.gas_price_buffer_bps_out_of_range".to_string());
    }
    Ok(())
}

fn get_fee_policy_stored() -> Result<FeePolicyStored, String> {
    let stored = with_state(|state| state.fee_policy.get().clone());
    if stored.fee_ledger_canister.is_empty() {
        return Err("config.fee_ledger_missing".to_string());
    }
    principal_from_bytes(stored.fee_ledger_canister.as_slice())?;
    validate_gas_price_buffer_bps(stored.gas_price_buffer_bps)?;
    Ok(stored)
}

fn ceil_mul_ratio_u128(value: u128, numerator: u128, denominator: u128) -> u128 {
    if denominator == 0 {
        return u128::MAX;
    }
    let prod = value.saturating_mul(numerator);
    let add = denominator.saturating_sub(1);
    prod.saturating_add(add) / denominator
}

fn compute_total_fee_e8s(
    gas_limit: u64,
    charged_gas_price_wei: u128,
    cycle_fee_e8s: u64,
) -> Result<u128, String> {
    if charged_gas_price_wei == 0 {
        return Err("fee.quote_zero".to_string());
    }
    let gas_fee_wei = u128::from(gas_limit).saturating_mul(charged_gas_price_wei);
    let gas_fee_e8s = gas_fee_wei.saturating_add(WEI_PER_E8S.saturating_sub(1)) / WEI_PER_E8S;
    Ok(gas_fee_e8s.saturating_add(u128::from(cycle_fee_e8s)))
}

fn map_fee_collection_error(code: String) -> String {
    if let Some(suffix) = code.strip_prefix("ledger.transfer_from_failed:") {
        return format!("fee.transfer_from_failed:{suffix}");
    }
    if let Some(suffix) = code.strip_prefix("ledger.decode_failed:") {
        return format!("fee.decode_failed:{suffix}");
    }
    if let Some(suffix) = code.strip_prefix("ledger.call_failed:") {
        return format!("fee.call_failed:{suffix}");
    }
    format!("fee.unknown:{code}")
}

fn principal_from_bytes(bytes: &[u8]) -> Result<Principal, String> {
    validate_principal_bytes(bytes)?;
    Ok(Principal::from_slice(bytes))
}

fn expected_evm_gateway_canister() -> Result<Principal, String> {
    let expected = with_state(|state| state.evm_gateway_canister.get().clone());
    if expected.is_empty() {
        return Err("config.evm_gateway_missing".to_string());
    }
    principal_from_bytes(expected.as_slice())
}

fn expected_evm_wrap_factory() -> Result<Vec<u8>, String> {
    let stored = with_state(|state| state.wrap_evm_config.get().clone());
    validate_evm_address(
        stored.wrap_factory_address.as_slice(),
        "config.wrap_factory_address_invalid",
    )?;
    Ok(stored.wrap_factory_address)
}

fn encode_factory_mint_for_asset_call_data(
    asset_id: &[u8],
    token_decimals: u8,
    recipient: &[u8],
    amount: &[u8],
) -> Result<Vec<u8>, String> {
    validate_principal_bytes(asset_id)?;
    validate_evm_address(recipient, "arg.evm_recipient_invalid")?;
    validate_amount_bytes(amount)?;
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

fn factory_mint_for_asset_selector() -> [u8; 4] {
    let mut keccak = Keccak::v256();
    keccak.update(b"mintForAsset(bytes,uint8,address,uint256)");
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    [out[0], out[1], out[2], out[3]]
}

fn to_request_id(bytes: &[u8]) -> Result<RequestId, String> {
    if bytes.len() != 32 {
        return Err("arg.request_id_invalid".to_string());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(bytes);
    Ok(RequestId(out))
}

impl RequestStatus {
    fn to_u8(self) -> u8 {
        match self {
            RequestStatus::Queued => 0,
            RequestStatus::Running => 1,
            RequestStatus::Succeeded => 2,
            RequestStatus::Failed => 3,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(RequestStatus::Queued),
            1 => Some(RequestStatus::Running),
            2 => Some(RequestStatus::Succeeded),
            3 => Some(RequestStatus::Failed),
            _ => None,
        }
    }
}

fn encode_stored_request(value: &StoredRequest) -> Option<Vec<u8>> {
    if !(1..=PRINCIPAL_MAX_BYTES).contains(&value.asset_id.len()) {
        return None;
    }
    if value.amount.len() != AMOUNT_BYTES {
        return None;
    }
    if !(1..=PRINCIPAL_MAX_BYTES).contains(&value.recipient.len()) {
        return None;
    }
    if value
        .result
        .ledger_tx_id
        .as_ref()
        .is_some_and(|v| v.len() > MAX_LEDGER_TX_ID_BYTES)
    {
        return None;
    }
    if value
        .result
        .error_code
        .as_ref()
        .is_some_and(|v| v.len() > MAX_ERROR_CODE_BYTES)
    {
        return None;
    }

    let mut out = Vec::with_capacity(STORED_REQUEST_MAX_BYTES as usize);
    out.push(1u8);
    write_u8_len_bytes(&mut out, &value.asset_id)?;
    out.extend_from_slice(&value.amount);
    write_u8_len_bytes(&mut out, &value.recipient)?;
    out.push(value.result.status.to_u8());
    match value.result.ledger_tx_id.as_ref() {
        Some(v) => {
            out.push(1u8);
            write_u8_len_bytes(&mut out, v)?;
        }
        None => out.push(0u8),
    }
    match value.result.error_code.as_ref() {
        Some(v) => {
            out.push(1u8);
            write_u8_len_bytes(&mut out, v.as_bytes())?;
        }
        None => out.push(0u8),
    }
    Some(out)
}

fn decode_stored_request(bytes: &[u8]) -> Option<StoredRequest> {
    let mut offset = 0usize;
    let version = *bytes.get(offset)?;
    offset += 1;
    let asset_id = read_u8_len_bytes(bytes, &mut offset, PRINCIPAL_MAX_BYTES)?;
    let amount_end = offset.checked_add(AMOUNT_BYTES)?;
    let amount = bytes.get(offset..amount_end)?.to_vec();
    offset = amount_end;
    let recipient = read_u8_len_bytes(bytes, &mut offset, PRINCIPAL_MAX_BYTES)?;
    if version != 1 {
        return None;
    }
    let status = RequestStatus::from_u8(*bytes.get(offset)?)?;
    offset += 1;

    let ledger_tx_id = match *bytes.get(offset)? {
        0 => {
            offset += 1;
            None
        }
        1 => {
            offset += 1;
            Some(read_u8_len_bytes(
                bytes,
                &mut offset,
                MAX_LEDGER_TX_ID_BYTES,
            )?)
        }
        _ => return None,
    };
    let error_code = match *bytes.get(offset)? {
        0 => {
            offset += 1;
            None
        }
        1 => {
            offset += 1;
            let raw = read_u8_len_bytes(bytes, &mut offset, MAX_ERROR_CODE_BYTES)?;
            Some(String::from_utf8(raw).ok()?)
        }
        _ => return None,
    };
    if offset != bytes.len() {
        return None;
    }
    Some(StoredRequest {
        asset_id,
        amount,
        recipient,
        result: RequestResult {
            status,
            ledger_tx_id,
            error_code,
            dispatch_status: Some(RequestDispatchStatusView::Dispatched),
            dispatch_error: None,
        },
    })
}

fn write_u8_len_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Option<()> {
    let len = u8::try_from(bytes.len()).ok()?;
    if len == 0 {
        return None;
    }
    out.push(len);
    out.extend_from_slice(bytes);
    Some(())
}

fn read_u8_len_bytes(data: &[u8], offset: &mut usize, max_len: usize) -> Option<Vec<u8>> {
    let len = *data.get(*offset)? as usize;
    if len == 0 || len > max_len {
        return None;
    }
    *offset = offset.checked_add(1)?;
    let end = offset.checked_add(len)?;
    let out = data.get(*offset..end)?.to_vec();
    *offset = end;
    Some(out)
}

#[ic_cdk::query]
fn export_did() -> String {
    candid::export_service!();
    __export_service()
}

#[cfg(test)]
mod tests {
    use super::{
        apply_insert_request_outcome, apply_runtime_config, approval_required_for_readiness,
        decode_asset_decimals,
        decode_stored_request, decode_u256_be, dequeue_request, derive_wrap_request_id,
        encode_factory_mint_for_asset_call_data, encode_stored_request, enqueue_request,
        init_state, insert_request, insert_wrap_request, is_withdrawable, map_transfer_reply,
        mark_request_running, mark_wrap_request_running, nat_from_32_be, nat_to_be_bytes,
        normalize_submit_wrap_args, on_worker_queue_drain, on_wrap_worker_queue_drain,
        principal_from_bytes, recover_request_state_after_upgrade,
        recover_wrap_request_state_after_upgrade, schedule_worker, schedule_wrap_worker,
        submit_error_to_code, to_request_id, to_withdraw_error_code, transfer_error_to_code,
        transfer_from_error_to_code, u256_from_u64, validate_non_anonymous_principal,
        validate_withdraw_request, with_state, with_state_mut, FeeCharge, Icrc1MetadataValue,
        Icrc1TransferError, Icrc2TransferFromError, InitArgs, InsertRequestOutcome,
        NormalizedDispatchUnwrapRequest, NormalizedSubmitWrapRequest, QueueMeta, RequestResult,
        RequestStatus, StoredRequest, SubmitTxError, SubmitWrapRequestArgs, UnwrapReadiness,
        WrapRequestResult, WrapStoredRequest, WORKER_SCHEDULED, WRAP_WORKER_SCHEDULED,
    };
    use candid::{decode_one, encode_one, Nat, Principal};
    use num_bigint::BigUint;

    fn reset_state() {
        init_state();
        with_state_mut(|state| {
            let request_keys: Vec<_> = state.requests.range(..).map(|entry| entry.0).collect();
            for key in request_keys {
                state.requests.remove(&key);
            }
            let queue_keys: Vec<_> = state.queue.range(..).map(|entry| entry.0).collect();
            for key in queue_keys {
                state.queue.remove(&key);
            }
            let wrap_request_keys: Vec<_> =
                state.wrap_requests.range(..).map(|entry| entry.0).collect();
            for key in wrap_request_keys {
                state.wrap_requests.remove(&key);
            }
            let wrap_queue_keys: Vec<_> = state.wrap_queue.range(..).map(|entry| entry.0).collect();
            for key in wrap_queue_keys {
                state.wrap_queue.remove(&key);
            }
            state
                .queue_meta
                .set(super::QueueMeta::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: queue_meta"));
            state
                .wrap_queue_meta
                .set(super::QueueMeta::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: wrap_queue_meta"));
            state
                .kasane_canister
                .set(Vec::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: kasane_canister"));
            state
                .evm_gateway_canister
                .set(Vec::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: evm_gateway_canister"));
            state
                .fee_policy
                .set(super::FeePolicyStored {
                    fee_ledger_canister: Vec::new(),
                    cycle_fee_e8s: super::DEFAULT_CYCLE_FEE_E8S,
                    gas_price_buffer_bps: super::DEFAULT_GAS_PRICE_BUFFER_BPS,
                })
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: fee_policy"));
            state
                .wrap_evm_config
                .set(super::WrapEvmConfigStored {
                    wrap_factory_address: Vec::new(),
                })
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: wrap_evm_config"));
        });
        super::PENDING_WRAP_SUBMISSIONS.with(|pending| {
            pending.borrow_mut().clear();
        });
    }

    fn sample_init_args(seed: u8, factory: [u8; 20]) -> InitArgs {
        InitArgs {
            kasane_canister: Principal::self_authenticating(&[seed, 1]),
            evm_gateway_canister: Principal::self_authenticating(&[seed, 2]),
            fee_ledger_canister: Principal::self_authenticating(&[seed, 3]),
            wrap_factory_address: factory.to_vec(),
            cycle_fee_e8s: u64::from(seed) + 1_000,
            gas_price_buffer_bps: 12_000 + u32::from(seed),
        }
    }

    fn no_schedule() {}

    fn test_fee_charge() -> FeeCharge {
        FeeCharge {
            ledger_tx_id: vec![0x44, 0x55],
            charged_fee_e8s: 1_000_000,
            charged_gas_price_wei: 300_000_000_000,
        }
    }

    fn sample_unwrap_args(request_id: [u8; 32]) -> NormalizedDispatchUnwrapRequest {
        NormalizedDispatchUnwrapRequest {
            request_id: request_id.to_vec(),
            asset_id: vec![2u8; 29],
            amount: vec![0u8; 32],
            recipient: vec![3u8; 29],
        }
    }

    fn sample_request_result(status: RequestStatus) -> RequestResult {
        RequestResult {
            status,
            ledger_tx_id: None,
            error_code: None,
            dispatch_status: Some(super::RequestDispatchStatusView::Dispatched),
            dispatch_error: None,
        }
    }

    fn sample_stored_request(status: RequestStatus) -> StoredRequest {
        StoredRequest {
            asset_id: vec![2u8; 29],
            amount: vec![0u8; 32],
            recipient: vec![3u8; 29],
            result: sample_request_result(status),
        }
    }

    fn sample_failed_unwrap_request_for(recipient: Principal) -> StoredRequest {
        StoredRequest {
            asset_id: vec![2u8; 29],
            amount: vec![0u8; 32],
            recipient: recipient.as_slice().to_vec(),
            result: RequestResult {
                status: RequestStatus::Failed,
                ledger_tx_id: None,
                error_code: Some("ledger.call_failed:oops".to_string()),
                dispatch_status: Some(super::RequestDispatchStatusView::Dispatched),
                dispatch_error: None,
            },
        }
    }

    #[test]
    fn nat_from_32_be_keeps_high_bits() {
        let mut amount = [0u8; 32];
        amount[0] = 1;
        let nat = nat_from_32_be(&amount).expect("valid");
        assert_eq!(nat.0.bits(), 249);
    }

    #[test]
    fn principal_from_bytes_rejects_too_long() {
        let err = principal_from_bytes(&[7u8; 30]).expect_err("must reject");
        assert_eq!(err, "arg.principal_invalid");
    }

    #[test]
    fn validate_non_anonymous_principal_rejects_anonymous() {
        let err = validate_non_anonymous_principal(
            &Principal::anonymous(),
            "arg.kasane_canister_anonymous",
        )
        .expect_err("must reject anonymous");
        assert_eq!(err, "arg.kasane_canister_anonymous");

        let err = validate_non_anonymous_principal(
            &Principal::anonymous(),
            "arg.fee_ledger_canister_anonymous",
        )
        .expect_err("must reject anonymous");
        assert_eq!(err, "arg.fee_ledger_canister_anonymous");
    }

    #[test]
    fn apply_runtime_config_overwrites_all_runtime_settings() {
        reset_state();
        apply_runtime_config(sample_init_args(1, [0x11; 20]));
        apply_runtime_config(sample_init_args(9, [0x99; 20]));

        let (kasane, gateway, fee_ledger, cycle_fee, gas_buffer, factory) = with_state(|state| {
            let fee_policy = state.fee_policy.get().clone();
            let wrap_config = state.wrap_evm_config.get().clone();
            (
                principal_from_bytes(state.kasane_canister.get()).expect("kasane principal"),
                principal_from_bytes(state.evm_gateway_canister.get()).expect("gateway principal"),
                principal_from_bytes(fee_policy.fee_ledger_canister.as_slice())
                    .expect("fee ledger principal"),
                fee_policy.cycle_fee_e8s,
                fee_policy.gas_price_buffer_bps,
                wrap_config.wrap_factory_address,
            )
        });

        let expected = sample_init_args(9, [0x99; 20]);
        assert_eq!(kasane, expected.kasane_canister);
        assert_eq!(gateway, expected.evm_gateway_canister);
        assert_eq!(fee_ledger, expected.fee_ledger_canister);
        assert_eq!(cycle_fee, expected.cycle_fee_e8s);
        assert_eq!(gas_buffer, expected.gas_price_buffer_bps);
        assert_eq!(factory, expected.wrap_factory_address);
    }

    #[test]
    fn insert_request_is_idempotent_for_same_payload() {
        reset_state();
        let args = sample_unwrap_args([1u8; 32]);
        let first = insert_request(args.clone()).expect("first should pass");
        assert_eq!(
            first,
            InsertRequestOutcome::Inserted(to_request_id(&[1u8; 32]).expect("id"))
        );
        let second = insert_request(args).expect("second should be idempotent");
        assert_eq!(
            second,
            InsertRequestOutcome::AlreadyExists(to_request_id(&[1u8; 32]).expect("id"))
        );
        let status = with_state(|state| {
            state
                .requests
                .get(&to_request_id(&[1u8; 32]).expect("id"))
                .map(|r| r.result.status)
        });
        assert_eq!(status, Some(RequestStatus::Queued));
        assert_eq!(with_state(|state| state.requests.len()), 1);
    }

    #[test]
    fn insert_request_rejects_duplicate_with_asset_mismatch() {
        reset_state();
        let args = sample_unwrap_args([1u8; 32]);
        insert_request(args).expect("first should pass");
        let err = insert_request(NormalizedDispatchUnwrapRequest {
            asset_id: vec![9u8; 29],
            ..sample_unwrap_args([1u8; 32])
        })
        .expect_err("asset mismatch should fail");
        assert_eq!(err, "request.idempotency_mismatch");
    }

    #[test]
    fn insert_request_rejects_duplicate_with_amount_mismatch() {
        reset_state();
        let args = sample_unwrap_args([1u8; 32]);
        insert_request(args).expect("first should pass");
        let err = insert_request(NormalizedDispatchUnwrapRequest {
            amount: vec![8u8; 32],
            ..sample_unwrap_args([1u8; 32])
        })
        .expect_err("amount mismatch should fail");
        assert_eq!(err, "request.idempotency_mismatch");
    }

    #[test]
    fn insert_request_rejects_duplicate_with_recipient_mismatch() {
        reset_state();
        let args = sample_unwrap_args([1u8; 32]);
        insert_request(args).expect("first should pass");
        let err = insert_request(NormalizedDispatchUnwrapRequest {
            recipient: vec![7u8; 29],
            ..sample_unwrap_args([1u8; 32])
        })
        .expect_err("recipient mismatch should fail");
        assert_eq!(err, "request.idempotency_mismatch");
    }

    #[test]
    fn submit_unwrap_request_does_not_requeue_existing_request() {
        reset_state();
        with_state_mut(|state| {
            let _ = state
                .kasane_canister
                .set(Principal::anonymous().as_slice().to_vec());
            let request_id = to_request_id(&[1u8; 32]).expect("id");
            state
                .requests
                .insert(request_id, sample_stored_request(RequestStatus::Queued));
            let mut meta = *state.queue_meta.get();
            let seq = meta.tail;
            meta.tail = meta.tail.saturating_add(1);
            state.queue.insert(seq, request_id);
            state
                .queue_meta
                .set(meta)
                .unwrap_or_else(|_| panic!("queue meta set failed"));
        });

        let request_id = apply_insert_request_outcome(
            InsertRequestOutcome::AlreadyExists(to_request_id(&[1u8; 32]).expect("id")),
            no_schedule,
        );
        assert_eq!(request_id, to_request_id(&[1u8; 32]).expect("id"));
        with_state(|state| {
            assert_eq!(state.requests.len(), 1);
            assert_eq!(state.queue.len(), 1);
            let req = state
                .requests
                .get(&to_request_id(&[1u8; 32]).expect("id"))
                .expect("request");
            assert_eq!(req.result.status, RequestStatus::Queued);
        });
    }

    #[test]
    fn apply_insert_request_outcome_enqueues_new_request_once() {
        reset_state();
        let request_id = to_request_id(&[9u8; 32]).expect("id");
        with_state_mut(|state| {
            state
                .requests
                .insert(request_id, sample_stored_request(RequestStatus::Queued));
        });

        let returned =
            apply_insert_request_outcome(InsertRequestOutcome::Inserted(request_id), no_schedule);
        assert_eq!(returned, request_id);
        with_state(|state| {
            assert_eq!(state.requests.len(), 1);
            assert_eq!(state.queue.len(), 1);
        });
    }

    #[test]
    fn submit_unwrap_request_keeps_queue_size_for_all_existing_statuses() {
        let statuses = [
            RequestStatus::Queued,
            RequestStatus::Running,
            RequestStatus::Succeeded,
            RequestStatus::Failed,
        ];
        for status in statuses {
            reset_state();
            with_state_mut(|state| {
                let _ = state
                    .kasane_canister
                    .set(Principal::anonymous().as_slice().to_vec());
                let request_id = to_request_id(&[status as u8 + 1; 32]).expect("id");
                state
                    .requests
                    .insert(request_id, sample_stored_request(status));
                if status == RequestStatus::Queued {
                    let mut meta = *state.queue_meta.get();
                    let seq = meta.tail;
                    meta.tail = meta.tail.saturating_add(1);
                    state.queue.insert(seq, request_id);
                    state
                        .queue_meta
                        .set(meta)
                        .unwrap_or_else(|_| panic!("queue meta set failed"));
                }
            });

            let request_id = apply_insert_request_outcome(
                InsertRequestOutcome::AlreadyExists(
                    to_request_id(&[status as u8 + 1; 32]).expect("id"),
                ),
                no_schedule,
            );
            assert_eq!(
                request_id,
                to_request_id(&[status as u8 + 1; 32]).expect("id")
            );
            with_state(|state| {
                assert_eq!(state.requests.len(), 1, "{status:?}");
                let expected_queue_len = u64::from(status == RequestStatus::Queued);
                assert_eq!(state.queue.len(), expected_queue_len, "{status:?}");
                let req = state
                    .requests
                    .get(&to_request_id(&[status as u8 + 1; 32]).expect("id"))
                    .expect("request");
                assert_eq!(req.result.status, status, "{status:?}");
            });
        }
    }

    #[test]
    fn stored_request_codec_roundtrip() {
        let req = StoredRequest {
            asset_id: vec![1u8; 29],
            amount: vec![2u8; 32],
            recipient: vec![3u8; 29],
            result: RequestResult {
                status: RequestStatus::Succeeded,
                ledger_tx_id: Some(vec![4u8; 16]),
                error_code: None,
                dispatch_status: Some(super::RequestDispatchStatusView::Dispatched),
                dispatch_error: None,
            },
        };
        let encoded = encode_stored_request(&req).expect("encode");
        let decoded = decode_stored_request(&encoded).expect("decode");
        assert_eq!(decoded.asset_id, req.asset_id);
        assert_eq!(decoded.amount, req.amount);
        assert_eq!(decoded.recipient, req.recipient);
        assert_eq!(decoded.result.status, RequestStatus::Succeeded);
        assert_eq!(decoded.result.ledger_tx_id, Some(vec![4u8; 16]));
    }

    #[test]
    fn stored_request_codec_rejects_invalid_length() {
        let req = StoredRequest {
            asset_id: vec![1u8; 30],
            amount: vec![2u8; 32],
            recipient: vec![3u8; 29],
            result: sample_request_result(RequestStatus::Queued),
        };
        assert!(encode_stored_request(&req).is_none());
        assert!(decode_stored_request(&[0xFF]).is_none());
    }

    #[test]
    fn recover_request_state_after_upgrade_requeues_running_request() {
        reset_state();
        let request_id = to_request_id(&[0x31u8; 32]).expect("id");
        with_state_mut(|state| {
            state
                .requests
                .insert(request_id, sample_stored_request(RequestStatus::Running));
            let mut meta = *state.queue_meta.get();
            let seq = meta.tail;
            meta.tail = meta.tail.saturating_add(1);
            state.queue.insert(seq, request_id);
            state
                .queue_meta
                .set(meta)
                .unwrap_or_else(|_| panic!("queue meta set failed"));
        });

        assert!(recover_request_state_after_upgrade(123));

        with_state(|state| {
            assert_eq!(state.queue.len(), 1);
            assert_eq!(state.queue_meta.get().head, 0);
            assert_eq!(state.queue_meta.get().tail, 1);
            assert_eq!(
                state.requests.get(&request_id).map(|req| req.result.status),
                Some(RequestStatus::Queued)
            );
            assert_eq!(state.queue.get(&0), Some(request_id));
        });
    }

    #[test]
    fn recover_request_state_after_upgrade_fills_missing_queued_request_once() {
        reset_state();
        let request_id = to_request_id(&[0x32u8; 32]).expect("id");
        with_state_mut(|state| {
            state
                .requests
                .insert(request_id, sample_stored_request(RequestStatus::Queued));
        });

        assert!(recover_request_state_after_upgrade(123));
        assert!(recover_request_state_after_upgrade(124));
        with_state(|state| {
            assert_eq!(state.queue.len(), 1);
            assert_eq!(state.queue_meta.get().tail, 1);
            assert_eq!(state.queue.get(&0), Some(request_id));
        });
    }

    #[test]
    fn recover_request_state_after_upgrade_keeps_terminal_requests_out_of_queue() {
        reset_state();
        let succeeded = to_request_id(&[0x33u8; 32]).expect("id");
        let failed = to_request_id(&[0x34u8; 32]).expect("id");
        with_state_mut(|state| {
            let mut succeeded_req = sample_stored_request(RequestStatus::Succeeded);
            succeeded_req.result.ledger_tx_id = Some(vec![1u8; 2]);
            state.requests.insert(succeeded, succeeded_req);

            let mut failed_req = sample_stored_request(RequestStatus::Failed);
            failed_req.result.error_code = Some("ledger.call_failed:oops".to_string());
            state.requests.insert(failed, failed_req);
        });

        assert!(!recover_request_state_after_upgrade(123));
        with_state(|state| {
            assert_eq!(state.queue.len(), 0);
            assert_eq!(
                state.requests.get(&succeeded).map(|req| req.result.status),
                Some(RequestStatus::Succeeded)
            );
            assert_eq!(
                state.requests.get(&failed).map(|req| req.result.status),
                Some(RequestStatus::Failed)
            );
        });
    }

    #[test]
    fn queue_dequeue_preserves_order() {
        reset_state();
        let a = to_request_id(&[1u8; 32]).expect("id");
        let b = to_request_id(&[2u8; 32]).expect("id");
        enqueue_request(a);
        enqueue_request(b);
        assert_eq!(dequeue_request(), Some(a));
        assert_eq!(dequeue_request(), Some(b));
        assert_eq!(dequeue_request(), None);
    }

    #[test]
    fn mark_request_running_sets_running_status() {
        reset_state();
        let request_id = to_request_id(&[1u8; 32]).expect("id");
        insert_request(NormalizedDispatchUnwrapRequest {
            amount: vec![3u8; 32],
            recipient: vec![4u8; 29],
            ..sample_unwrap_args(request_id.0)
        })
        .expect("insert");
        mark_request_running(request_id);
        let status = with_state(|state| state.requests.get(&request_id).map(|v| v.result.status));
        assert_eq!(status, Some(RequestStatus::Running));
    }

    #[test]
    fn apply_unwrap_transfer_result_marks_failure_without_requeue_shape() {
        let mut req = sample_stored_request(RequestStatus::Running);
        super::apply_unwrap_transfer_result(
            &mut req,
            Err("ledger.transfer_failed:temporarily_unavailable".to_string()),
        );
        assert_eq!(req.result.status, RequestStatus::Failed);
        assert_eq!(req.result.ledger_tx_id, None);
        assert_eq!(
            req.result.error_code.as_deref(),
            Some("ledger.transfer_failed:temporarily_unavailable")
        );
    }

    #[test]
    fn apply_unwrap_transfer_result_marks_success_shape() {
        let mut req = sample_stored_request(RequestStatus::Running);
        super::apply_unwrap_transfer_result(&mut req, Ok(vec![0x12, 0x34]));
        assert_eq!(req.result.status, RequestStatus::Succeeded);
        assert_eq!(req.result.ledger_tx_id, Some(vec![0x12, 0x34]));
        assert_eq!(req.result.error_code, None);
    }

    #[test]
    fn reserve_failed_unwrap_retry_requires_recipient_caller() {
        reset_state();
        let request_id = to_request_id(&[0x55u8; 32]).expect("id");
        let recipient = Principal::self_authenticating(b"unwrap-recipient");
        with_state_mut(|state| {
            state
                .requests
                .insert(request_id, sample_failed_unwrap_request_for(recipient));
        });

        let err = super::reserve_failed_unwrap_retry(
            request_id,
            Principal::self_authenticating(b"other-caller"),
        )
        .expect_err("non recipient must fail");
        assert_eq!(err, "unwrap.retry_not_recipient");
    }

    #[test]
    fn reserve_failed_unwrap_retry_marks_running_once() {
        reset_state();
        let request_id = to_request_id(&[0x56u8; 32]).expect("id");
        let recipient = Principal::self_authenticating(b"unwrap-recipient-running");
        with_state_mut(|state| {
            state
                .requests
                .insert(request_id, sample_failed_unwrap_request_for(recipient));
        });

        let reserved = super::reserve_failed_unwrap_retry(request_id, recipient).expect("reserve");
        assert_eq!(reserved.0, vec![2u8; 29]);
        assert_eq!(
            with_state(|state| state.requests.get(&request_id).map(|req| req.result.status)),
            Some(RequestStatus::Running)
        );
        let err =
            super::reserve_failed_unwrap_retry(request_id, recipient).expect_err("second reserve");
        assert_eq!(err, "unwrap.retry_already_running");
    }

    #[test]
    fn reserve_failed_unwrap_retry_rejects_terminal_success() {
        reset_state();
        let request_id = to_request_id(&[0x57u8; 32]).expect("id");
        let recipient = Principal::self_authenticating(b"unwrap-recipient-succeeded");
        let mut req = sample_failed_unwrap_request_for(recipient);
        req.result.status = RequestStatus::Succeeded;
        req.result.ledger_tx_id = Some(vec![0x99]);
        with_state_mut(|state| {
            state.requests.insert(request_id, req);
        });

        let err = super::reserve_failed_unwrap_retry(request_id, recipient).expect_err("succeeded");
        assert_eq!(err, "unwrap.retry_invalid_state");
    }

    #[test]
    fn nat_to_be_bytes_preserves_high_bit_width() {
        let value = Nat(BigUint::from(1u8) << 200usize);
        let encoded = nat_to_be_bytes(&value);
        assert!(encoded.len() > 16);
        assert_eq!(encoded.first().copied(), Some(1u8));
    }

    #[test]
    fn decode_u256_be_accepts_max_uint256() {
        let decoded = decode_u256_be(&[0xffu8; 32]).expect("must decode");
        assert_eq!(decoded, [0xffu8; 32]);
        let nat = Nat(BigUint::from_bytes_be(&decoded));
        assert!(nat > Nat::from(u128::MAX));
    }

    #[test]
    fn transfer_error_to_code_formats_expected_variant() {
        let code = transfer_error_to_code(&Icrc1TransferError::Duplicate {
            duplicate_of: Nat(BigUint::from(42u32)),
        });
        assert_eq!(code, "duplicate:42");
    }

    #[test]
    fn candid_roundtrip_for_icrc1_transfer_result_decodes_nat_and_error() {
        let ok_wire = encode_one((Ok::<Nat, Icrc1TransferError>(Nat(BigUint::from(7u32))),))
            .expect("encode ok");
        let ok_decoded: (Result<Nat, Icrc1TransferError>,) =
            decode_one(&ok_wire).expect("decode ok");
        let ok_mapped = map_transfer_reply(ok_decoded.0).expect("map ok");
        assert_eq!(ok_mapped, vec![7u8]);

        let err_wire = encode_one((Err::<Nat, Icrc1TransferError>(
            Icrc1TransferError::TemporarilyUnavailable,
        ),))
        .expect("encode err");
        let err_decoded: (Result<Nat, Icrc1TransferError>,) =
            decode_one(&err_wire).expect("decode err");
        let err = map_transfer_reply(err_decoded.0).expect_err("map err");
        assert_eq!(err, "ledger.transfer_failed:temporarily_unavailable");
    }

    #[test]
    fn dequeue_empty_keeps_queue_meta() {
        reset_state();
        let before = with_state(|state| *state.queue_meta.get());
        assert!(dequeue_request().is_none());
        let after = with_state(|state| *state.queue_meta.get());
        assert_eq!(before, QueueMeta::new());
        assert_eq!(after, before);
    }

    #[test]
    fn on_worker_queue_drain_clears_flag_when_queue_empty() {
        reset_state();
        WORKER_SCHEDULED.with(|f| f.set(true));
        on_worker_queue_drain();
        let scheduled = WORKER_SCHEDULED.with(|f| f.get());
        assert!(!scheduled);
    }

    #[test]
    fn schedule_worker_is_idempotent_when_already_scheduled() {
        reset_state();
        WORKER_SCHEDULED.with(|f| f.set(true));
        schedule_worker();
        let scheduled = WORKER_SCHEDULED.with(|f| f.get());
        assert!(scheduled);
    }

    #[test]
    fn wrap_insert_request_rejects_duplicate() {
        reset_state();
        let caller = Principal::self_authenticating(b"wrap-caller-dup");
        let asset_id = vec![2u8; 29];
        let amount = vec![0u8; 32];
        let evm_recipient = vec![4u8; 20];
        let request_id = derive_wrap_request_id(
            caller.as_slice(),
            asset_id.as_slice(),
            amount.as_slice(),
            evm_recipient.as_slice(),
            7,
            200_000,
        );
        let args = NormalizedSubmitWrapRequest {
            request_id: to_request_id(&request_id).expect("id"),
            asset_id,
            amount,
            evm_recipient,
            gas_limit: 200_000,
        };
        let request_id = to_request_id(&request_id).expect("id");
        insert_wrap_request(args.clone(), caller, request_id, test_fee_charge())
            .expect("first should pass");
        let err = insert_wrap_request(args, caller, request_id, test_fee_charge())
            .expect_err("second should fail");
        assert_eq!(err, "wrap.request.duplicate");
    }

    #[test]
    fn wrap_request_id_changes_when_evm_nonce_changes() {
        reset_state();
        let caller = Principal::self_authenticating(b"wrap-caller-nonce");
        let asset_id = vec![2u8; 29];
        let amount = vec![3u8; 32];
        let evm_recipient = vec![4u8; 20];

        let first = derive_wrap_request_id(
            caller.as_slice(),
            asset_id.as_slice(),
            amount.as_slice(),
            evm_recipient.as_slice(),
            10,
            200_000,
        );
        let second = derive_wrap_request_id(
            caller.as_slice(),
            asset_id.as_slice(),
            amount.as_slice(),
            evm_recipient.as_slice(),
            11,
            200_000,
        );

        assert_ne!(first, second);
    }

    #[test]
    fn approval_required_only_for_allowance_shortage() {
        assert!(!approval_required_for_readiness(UnwrapReadiness::Ready));
        assert!(!approval_required_for_readiness(
            UnwrapReadiness::TokenNotDeployed
        ));
        assert!(!approval_required_for_readiness(
            UnwrapReadiness::InsufficientBalance
        ));
        assert!(approval_required_for_readiness(
            UnwrapReadiness::InsufficientAllowance
        ));
    }

    #[test]
    fn mark_wrap_request_running_sets_running_status() {
        reset_state();
        let caller = Principal::self_authenticating(b"wrap-caller-running");
        let asset_id = vec![2u8; 29];
        let amount = vec![3u8; 32];
        let evm_recipient = vec![5u8; 20];
        let request_id_raw = derive_wrap_request_id(
            caller.as_slice(),
            asset_id.as_slice(),
            amount.as_slice(),
            evm_recipient.as_slice(),
            9,
            300_000,
        );
        let request_id = to_request_id(&request_id_raw).expect("id");
        insert_wrap_request(
            NormalizedSubmitWrapRequest {
                request_id,
                asset_id,
                amount,
                evm_recipient,
                gas_limit: 300_000,
            },
            caller,
            request_id,
            test_fee_charge(),
        )
        .expect("insert");
        mark_wrap_request_running(request_id);
        let status = with_state(|state| {
            state
                .wrap_requests
                .get(&request_id)
                .map(|v| (v.result.status, v.result.withdraw_in_progress))
        });
        assert_eq!(status, Some((RequestStatus::Running, false)));
    }

    #[test]
    fn wrap_normalize_submit_rejects_zero_gas_limit() {
        reset_state();
        let err = normalize_submit_wrap_args(SubmitWrapRequestArgs {
            asset_id: Principal::self_authenticating(b"wrap-asset-zero-gas"),
            amount_e8s: Nat::from(3u8),
            evm_recipient: vec![5u8; 20],
            evm_nonce: 0,
            gas_limit: 0,
        })
        .expect_err("zero gas limit must fail");
        assert_eq!(err, "arg.gas_limit_invalid");
    }

    #[test]
    fn transfer_from_error_to_code_formats_expected_variant() {
        let code = transfer_from_error_to_code(&Icrc2TransferFromError::InsufficientAllowance {
            allowance: Nat(BigUint::from(9u32)),
        });
        assert_eq!(code, "insufficient_allowance:9");
    }

    #[test]
    fn submit_error_to_code_formats_variant() {
        let code = submit_error_to_code(SubmitTxError::Rejected("nonce_low".to_string()));
        assert_eq!(code, "rejected:nonce_low");
    }

    #[test]
    fn build_submit_ic_tx_args_keeps_charge_as_max_fee_and_splits_priority_fee() {
        let charged_gas_price_wei = 300_000_000_000u128;
        let suggested_priority_fee_wei = 150_000_000_000u128;
        let args = super::build_submit_ic_tx_args(
            vec![0x11; 20],
            7,
            450_000,
            charged_gas_price_wei,
            suggested_priority_fee_wei,
            vec![0xaa, 0xbb],
        );

        assert_eq!(args.to, Some(vec![0x11; 20]));
        assert_eq!(args.from, None);
        assert_eq!(args.gas_limit, 450_000);
        assert_eq!(args.nonce, 7);
        assert_eq!(
            super::nat_to_u128(&args.max_fee_per_gas),
            Some(charged_gas_price_wei)
        );
        assert_eq!(
            super::nat_to_u128(&args.max_priority_fee_per_gas),
            Some(suggested_priority_fee_wei)
        );
        assert_ne!(args.max_fee_per_gas, args.max_priority_fee_per_gas);
        assert_eq!(args.data, vec![0xaa, 0xbb]);
    }

    #[test]
    fn build_submit_ic_tx_args_caps_priority_fee_at_max_fee() {
        let charged_gas_price_wei = 300_000_000_000u128;
        let suggested_priority_fee_wei = 450_000_000_000u128;
        let args = super::build_submit_ic_tx_args(
            vec![0x11; 20],
            7,
            450_000,
            charged_gas_price_wei,
            suggested_priority_fee_wei,
            vec![0xaa, 0xbb],
        );

        assert_eq!(
            super::nat_to_u128(&args.max_fee_per_gas),
            Some(charged_gas_price_wei)
        );
        assert_eq!(
            super::nat_to_u128(&args.max_priority_fee_per_gas),
            Some(charged_gas_price_wei)
        );
    }

    #[test]
    fn compute_total_fee_e8s_keeps_existing_charge_formula() {
        let charged =
            super::compute_total_fee_e8s(21_000, 300_000_000_000, 1_000_000).expect("fee");
        let expected_gas_fee_e8s = (21_000u128 * 300_000_000_000u128).div_ceil(super::WEI_PER_E8S);
        assert_eq!(charged, expected_gas_fee_e8s + 1_000_000u128);
    }

    #[test]
    fn encode_factory_mint_for_asset_call_data_encodes_selector_and_words() {
        let data =
            encode_factory_mint_for_asset_call_data(&[0x33u8; 29], 8, &[0x11u8; 20], &[0x22u8; 32])
                .expect("encode");
        assert_eq!(data.len(), 196);
        assert_ne!(&data[0..4], &[0u8; 4]);
        assert_eq!(&data[4..36], &u256_from_u64(128));
        assert_eq!(&data[36..68], &u256_from_u64(8));
        assert_eq!(&data[68..80], &[0u8; 12]);
        assert_eq!(&data[80..100], &[0x11u8; 20]);
        assert_eq!(&data[100..132], &[0x22u8; 32]);
        assert_eq!(&data[132..164], &u256_from_u64(29));
        assert_eq!(&data[164..193], &[0x33u8; 29]);
    }

    #[test]
    fn decode_asset_decimals_reads_nat_value() {
        let decimals = decode_asset_decimals(&[
            (
                "icrc1:name".to_string(),
                Icrc1MetadataValue::Text("Token".to_string()),
            ),
            (
                "icrc1:decimals".to_string(),
                Icrc1MetadataValue::Nat(Nat::from(8u8)),
            ),
        ])
        .expect("decimals");
        assert_eq!(decimals, 8);
    }

    #[test]
    fn decode_asset_decimals_rejects_missing_or_invalid_value() {
        let missing = decode_asset_decimals(&[(
            "icrc1:name".to_string(),
            Icrc1MetadataValue::Text("Token".to_string()),
        )])
        .expect_err("missing");
        assert_eq!(missing, "wrap.asset_metadata_failed:decimals_missing");

        let invalid = decode_asset_decimals(&[(
            "icrc1:decimals".to_string(),
            Icrc1MetadataValue::Text("8".to_string()),
        )])
        .expect_err("invalid");
        assert_eq!(invalid, "wrap.asset_decimals_invalid");
    }

    #[test]
    fn wrap_request_result_candid_roundtrip_keeps_withdraw_fields() {
        let value = WrapRequestResult {
            status: RequestStatus::Failed,
            pull_ledger_tx_id: Some(vec![1u8; 4]),
            mint_tx_id: None,
            error_code: Some("mint_failed".to_string()),
            withdrawn: true,
            withdraw_ledger_tx_id: Some(vec![2u8; 4]),
            withdraw_error_code: Some("withdraw.call_failed:oops".to_string()),
            withdraw_in_progress: true,
            mint_failed_recoverable: false,
            fee_ledger_tx_id: Some(vec![3u8; 4]),
            charged_fee_e8s: Some(1_000_000),
            charged_gas_price_wei: Some(300_000_000_000),
        };
        let bytes = encode_one(&value).expect("encode");
        let decoded: WrapRequestResult = decode_one(&bytes).expect("decode");
        assert!(decoded.withdrawn);
        assert_eq!(decoded.withdraw_ledger_tx_id, Some(vec![2u8; 4]));
        assert_eq!(
            decoded.withdraw_error_code.as_deref(),
            Some("withdraw.call_failed:oops")
        );
        assert!(decoded.withdraw_in_progress);
    }

    #[test]
    fn mint_failed_recoverable_is_set_on_mint_failure_outcome_shape() {
        let req = WrapStoredRequest {
            caller: candid::Principal::self_authenticating(b"wrap-test-caller")
                .as_slice()
                .to_vec(),
            asset_id: vec![7u8; 29],
            amount: vec![0u8; 32],
            evm_recipient: vec![9u8; 20],
            gas_limit: 300_000,
            result: WrapRequestResult {
                status: RequestStatus::Failed,
                pull_ledger_tx_id: Some(vec![1u8; 4]),
                mint_tx_id: None,
                error_code: Some("evm_gateway.submit_failed:rejected:nonce".to_string()),
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                withdraw_in_progress: false,
                mint_failed_recoverable: true,
                fee_ledger_tx_id: Some(vec![3u8; 4]),
                charged_fee_e8s: Some(1_000_000),
                charged_gas_price_wei: Some(300_000_000_000),
            },
        };
        assert!(req.result.mint_failed_recoverable);
        assert!(req.result.pull_ledger_tx_id.is_some());
    }

    #[test]
    fn validate_withdraw_request_checks_owner_and_state() {
        let owner = candid::Principal::self_authenticating(b"wrap-owner");
        let other = candid::Principal::self_authenticating(b"wrap-other");
        let base = WrapStoredRequest {
            caller: owner.as_slice().to_vec(),
            asset_id: vec![7u8; 29],
            amount: vec![0u8; 32],
            evm_recipient: vec![9u8; 20],
            gas_limit: 300_000,
            result: WrapRequestResult {
                status: RequestStatus::Failed,
                pull_ledger_tx_id: Some(vec![1u8; 4]),
                mint_tx_id: None,
                error_code: Some("mint_failed".to_string()),
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                withdraw_in_progress: false,
                mint_failed_recoverable: true,
                fee_ledger_tx_id: Some(vec![3u8; 4]),
                charged_fee_e8s: Some(1_000_000),
                charged_gas_price_wei: Some(300_000_000_000),
            },
        };
        validate_withdraw_request(&base, owner).expect("eligible");
        let not_owner = validate_withdraw_request(&base, other).expect_err("owner check");
        assert_eq!(not_owner, "withdraw.not_request_owner");

        let mut non_recoverable = base.clone();
        non_recoverable.result.mint_failed_recoverable = false;
        let invalid = validate_withdraw_request(&non_recoverable, owner).expect_err("state");
        assert_eq!(invalid, "withdraw.invalid_state");

        let mut withdrawn = base.clone();
        withdrawn.result.withdrawn = true;
        let already = validate_withdraw_request(&withdrawn, owner).expect_err("withdrawn");
        assert_eq!(already, "withdraw.already_withdrawn");

        let mut in_progress = base;
        in_progress.result.withdraw_in_progress = true;
        let blocked = validate_withdraw_request(&in_progress, owner).expect_err("in progress");
        assert_eq!(blocked, "withdraw.in_progress");
    }

    #[test]
    fn is_withdrawable_matches_expected_shape() {
        let owner = Principal::self_authenticating(b"wrap-owner");
        let req = WrapStoredRequest {
            caller: owner.as_slice().to_vec(),
            asset_id: vec![7u8; 29],
            amount: vec![0u8; 32],
            evm_recipient: vec![9u8; 20],
            gas_limit: 300_000,
            result: WrapRequestResult {
                status: RequestStatus::Failed,
                pull_ledger_tx_id: Some(vec![1u8; 4]),
                mint_tx_id: None,
                error_code: Some("mint_failed".to_string()),
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                withdraw_in_progress: false,
                mint_failed_recoverable: true,
                fee_ledger_tx_id: Some(vec![3u8; 4]),
                charged_fee_e8s: Some(1_000_000),
                charged_gas_price_wei: Some(300_000_000_000),
            },
        };
        assert!(is_withdrawable(&req));
    }

    #[test]
    fn withdraw_error_code_mapping_is_stable() {
        assert_eq!(
            to_withdraw_error_code("ledger.transfer_failed:insufficient_funds:1"),
            "withdraw.transfer_failed:insufficient_funds:1"
        );
        assert_eq!(
            to_withdraw_error_code("ledger.decode_failed:bad wire"),
            "withdraw.decode_failed:bad wire"
        );
        assert_eq!(
            to_withdraw_error_code("ledger.call_failed:canister reject"),
            "withdraw.call_failed:canister reject"
        );
    }

    #[test]
    fn on_wrap_worker_queue_drain_clears_flag_when_queue_empty() {
        reset_state();
        WRAP_WORKER_SCHEDULED.with(|f| f.set(true));
        on_wrap_worker_queue_drain();
        let scheduled = WRAP_WORKER_SCHEDULED.with(|f| f.get());
        assert!(!scheduled);
    }

    #[test]
    fn schedule_wrap_worker_is_idempotent_when_already_scheduled() {
        reset_state();
        WRAP_WORKER_SCHEDULED.with(|f| f.set(true));
        schedule_wrap_worker();
        let scheduled = WRAP_WORKER_SCHEDULED.with(|f| f.get());
        assert!(scheduled);
    }

    #[test]
    fn recover_wrap_request_state_after_upgrade_requeues_running_request() {
        reset_state();
        let request_id = to_request_id(&[0x41u8; 32]).expect("id");
        with_state_mut(|state| {
            state.wrap_requests.insert(
                request_id,
                WrapStoredRequest {
                    caller: Principal::self_authenticating(b"wrap-running")
                        .as_slice()
                        .to_vec(),
                    asset_id: vec![7u8; 29],
                    amount: vec![8u8; 32],
                    evm_recipient: vec![9u8; 20],
                    gas_limit: 300_000,
                    result: WrapRequestResult {
                        status: RequestStatus::Running,
                        pull_ledger_tx_id: None,
                        mint_tx_id: None,
                        error_code: None,
                        withdrawn: false,
                        withdraw_ledger_tx_id: None,
                        withdraw_error_code: None,
                        withdraw_in_progress: false,
                        mint_failed_recoverable: false,
                        fee_ledger_tx_id: Some(vec![3u8; 4]),
                        charged_fee_e8s: Some(1_000_000),
                        charged_gas_price_wei: Some(300_000_000_000),
                    },
                },
            );
        });

        assert!(recover_wrap_request_state_after_upgrade(123));
        with_state(|state| {
            let req = state.wrap_requests.get(&request_id).expect("request");
            assert_eq!(req.result.status, RequestStatus::Queued);
            assert_eq!(req.result.fee_ledger_tx_id, Some(vec![3u8; 4]));
            assert_eq!(req.result.charged_fee_e8s, Some(1_000_000));
            assert_eq!(req.result.charged_gas_price_wei, Some(300_000_000_000));
            assert_eq!(state.wrap_queue.len(), 1);
            assert_eq!(state.wrap_queue_meta.get().tail, 1);
            assert_eq!(state.wrap_queue.get(&0), Some(request_id));
        });
    }

    #[test]
    fn recover_wrap_request_state_after_upgrade_does_not_duplicate_existing_queue_entry() {
        reset_state();
        let request_id = to_request_id(&[0x42u8; 32]).expect("id");
        with_state_mut(|state| {
            state.wrap_requests.insert(
                request_id,
                WrapStoredRequest {
                    caller: Principal::self_authenticating(b"wrap-queued")
                        .as_slice()
                        .to_vec(),
                    asset_id: vec![7u8; 29],
                    amount: vec![8u8; 32],
                    evm_recipient: vec![9u8; 20],
                    gas_limit: 300_000,
                    result: WrapRequestResult {
                        status: RequestStatus::Queued,
                        pull_ledger_tx_id: None,
                        mint_tx_id: None,
                        error_code: None,
                        withdrawn: false,
                        withdraw_ledger_tx_id: None,
                        withdraw_error_code: None,
                        withdraw_in_progress: false,
                        mint_failed_recoverable: false,
                        fee_ledger_tx_id: Some(vec![3u8; 4]),
                        charged_fee_e8s: Some(1_000_000),
                        charged_gas_price_wei: Some(300_000_000_000),
                    },
                },
            );
            let mut meta = *state.wrap_queue_meta.get();
            let seq = meta.tail;
            meta.tail = meta.tail.saturating_add(1);
            state.wrap_queue.insert(seq, request_id);
            state
                .wrap_queue_meta
                .set(meta)
                .unwrap_or_else(|_| panic!("wrap queue meta set failed"));
        });

        assert!(recover_wrap_request_state_after_upgrade(123));
        with_state(|state| {
            assert_eq!(state.wrap_queue.len(), 1);
            assert_eq!(state.wrap_queue_meta.get().tail, 1);
            assert_eq!(state.wrap_queue.get(&0), Some(request_id));
        });
    }

    #[test]
    fn recover_wrap_request_state_after_upgrade_keeps_terminal_requests_out_of_queue() {
        reset_state();
        let request_id = to_request_id(&[0x43u8; 32]).expect("id");
        with_state_mut(|state| {
            state.wrap_requests.insert(
                request_id,
                WrapStoredRequest {
                    caller: Principal::self_authenticating(b"wrap-failed")
                        .as_slice()
                        .to_vec(),
                    asset_id: vec![7u8; 29],
                    amount: vec![8u8; 32],
                    evm_recipient: vec![9u8; 20],
                    gas_limit: 300_000,
                    result: WrapRequestResult {
                        status: RequestStatus::Failed,
                        pull_ledger_tx_id: Some(vec![1u8; 4]),
                        mint_tx_id: None,
                        error_code: Some("evm_gateway.submit_failed:rejected:nonce".to_string()),
                        withdrawn: false,
                        withdraw_ledger_tx_id: None,
                        withdraw_error_code: None,
                        withdraw_in_progress: false,
                        mint_failed_recoverable: true,
                        fee_ledger_tx_id: Some(vec![3u8; 4]),
                        charged_fee_e8s: Some(1_000_000),
                        charged_gas_price_wei: Some(300_000_000_000),
                    },
                },
            );
        });

        assert!(!recover_wrap_request_state_after_upgrade(123));
        with_state(|state| {
            let req = state.wrap_requests.get(&request_id).expect("request");
            assert_eq!(req.result.status, RequestStatus::Failed);
            assert_eq!(req.result.pull_ledger_tx_id, Some(vec![1u8; 4]));
            assert_eq!(req.result.mint_failed_recoverable, true);
            assert_eq!(state.wrap_queue.len(), 0);
        });
    }

    #[test]
    fn recover_wrap_request_state_after_upgrade_clears_withdraw_in_progress() {
        reset_state();
        let request_id = to_request_id(&[0x44u8; 32]).expect("id");
        with_state_mut(|state| {
            state.wrap_requests.insert(
                request_id,
                WrapStoredRequest {
                    caller: Principal::self_authenticating(b"wrap-withdraw-in-progress")
                        .as_slice()
                        .to_vec(),
                    asset_id: vec![7u8; 29],
                    amount: vec![8u8; 32],
                    evm_recipient: vec![9u8; 20],
                    gas_limit: 300_000,
                    result: WrapRequestResult {
                        status: RequestStatus::Failed,
                        pull_ledger_tx_id: Some(vec![1u8; 4]),
                        mint_tx_id: None,
                        error_code: Some("recover_failed".to_string()),
                        withdrawn: false,
                        withdraw_ledger_tx_id: None,
                        withdraw_error_code: None,
                        withdraw_in_progress: true,
                        mint_failed_recoverable: true,
                        fee_ledger_tx_id: Some(vec![3u8; 4]),
                        charged_fee_e8s: Some(1_000_000),
                        charged_gas_price_wei: Some(300_000_000_000),
                    },
                },
            );
        });

        assert!(!recover_wrap_request_state_after_upgrade(123));
        with_state(|state| {
            let req = state.wrap_requests.get(&request_id).expect("request");
            assert!(!req.result.withdraw_in_progress);
            assert_eq!(req.result.status, RequestStatus::Failed);
            assert_eq!(state.wrap_queue.len(), 0);
        });
    }
}
