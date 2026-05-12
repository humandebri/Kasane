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
#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_keccak::{Hasher, Keccak};

mod icrc21;

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
const MEM_ALLOWED_ASSETS: MemoryId = MemoryId::new(8);
const MEM_FEE_POLICY: MemoryId = MemoryId::new(9);
const MEM_WRAP_EVM_CONFIG: MemoryId = MemoryId::new(10);
const MEM_NATIVE_LEDGER_CANISTER: MemoryId = MemoryId::new(11);
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
const MAX_CYCLE_FEE_E8S: u64 = 1_000_000_000_000;
const DEFAULT_GAS_PRICE_BUFFER_BPS: u32 = 12_000;
const GAS_PRICE_DENOMINATOR_BPS: u128 = 10_000;
const WEI_PER_E8S: u128 = 10_000_000_000;
type Memory = VirtualMemory<DefaultMemoryImpl>;
type RetryTransferReservation = (Vec<u8>, Vec<u8>, Vec<u8>);
type NormalizedSubmitWrapArgsParts = (Vec<u8>, Vec<u8>, Vec<u8>, u64, u64, u128, u128, Principal);

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub kasane_canister: Principal,
    pub evm_gateway_canister: Principal,
    pub fee_ledger_canister: Principal,
    pub native_ledger_canister: Principal,
    pub wrap_factory_address: Vec<u8>,
    pub cycle_fee_e8s: u64,
    pub gas_price_buffer_bps: u32,
    pub allowed_assets: Vec<Principal>,
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
pub struct DispatchNativeWithdrawalRequestArgs {
    pub request_id: Vec<u8>,
    pub amount_e8s: Nat,
    pub recipient: Principal,
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
    created_at_time: u64,
    result: RequestResult,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
struct WrapStoredRequest {
    caller: Vec<u8>,
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    evm_recipient: Vec<u8>,
    gas_limit: u64,
    #[serde(default)]
    fee_created_at_time: u64,
    #[serde(default)]
    pull_created_at_time: u64,
    #[serde(default)]
    withdraw_created_at_time: u64,
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
    max_fee_e8s: u128,
    quoted_gas_price_wei: u128,
    fee_ledger_canister: Principal,
}

#[derive(Clone, Debug)]
struct NormalizedSubmitNativeDeposit {
    request_id: RequestId,
    amount: Vec<u8>,
    evm_recipient: Vec<u8>,
    max_fee_e8s: u128,
    fee_ledger_canister: Principal,
}

#[derive(Clone, Debug)]
struct NormalizedQuoteWrapRequest {
    asset_id: Vec<u8>,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TransferMemoKind {
    Unwrap,
    Fee,
    Pull,
    Withdraw,
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

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
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

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
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

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
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

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
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

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
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

    fn into_bytes(self) -> Vec<u8> {
        self.to_bytes().into_owned()
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
    native_ledger_canister: StableCell<Vec<u8>, Memory>,
    fee_policy: StableCell<FeePolicyStored, Memory>,
    wrap_evm_config: StableCell<WrapEvmConfigStored, Memory>,
    allowed_assets: StableBTreeMap<Vec<u8>, u8, Memory>,
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

fn current_time_nanos() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        // canister本番では IC の単調増加ナノ秒時刻を使う。
        ic_cdk::api::time()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        // host test では SystemTime を使い、ic0::time 依存を避ける。
        // u64 へ収まらない値は clamp して stable payload の型に合わせる。
        let nanos_u128 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let clamped = nanos_u128.min(u128::from(u64::MAX));
        u64::try_from(clamped).unwrap_or(u64::MAX)
    }
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
        });
        let requests = with_memory(MEM_REQUESTS, StableBTreeMap::init);
        let queue = with_memory(MEM_QUEUE, StableBTreeMap::init);
        let queue_meta = with_memory(MEM_QUEUE_META, |memory| {
            StableCell::init(memory, QueueMeta::new())
        });
        let evm_gateway_canister = with_memory(MEM_EVM_GATEWAY_CANISTER, |memory| {
            StableCell::init(memory, Vec::<u8>::new())
        });
        let native_ledger_canister = with_memory(MEM_NATIVE_LEDGER_CANISTER, |memory| {
            StableCell::init(memory, Vec::<u8>::new())
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
        });
        let wrap_evm_config = with_memory(MEM_WRAP_EVM_CONFIG, |memory| {
            StableCell::init(
                memory,
                WrapEvmConfigStored {
                    wrap_factory_address: Vec::new(),
                },
            )
        });
        let allowed_assets = with_memory(MEM_ALLOWED_ASSETS, StableBTreeMap::init);
        let wrap_requests = with_memory(MEM_WRAP_REQUESTS, StableBTreeMap::init);
        let wrap_queue = with_memory(MEM_WRAP_QUEUE, StableBTreeMap::init);
        let wrap_queue_meta = with_memory(MEM_WRAP_QUEUE_META, |memory| {
            StableCell::init(memory, QueueMeta::new())
        });
        *cell.borrow_mut() = Some(StableState {
            kasane_canister,
            evm_gateway_canister,
            native_ledger_canister,
            fee_policy,
            wrap_evm_config,
            allowed_assets,
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
    let now = current_time_nanos();
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
        let _ = state
            .kasane_canister
            .set(args.kasane_canister.as_slice().to_vec());
        let _ = state
            .evm_gateway_canister
            .set(args.evm_gateway_canister.as_slice().to_vec());
        let _ = state
            .native_ledger_canister
            .set(args.native_ledger_canister.as_slice().to_vec());
        let _ = state.fee_policy.set(FeePolicyStored {
            fee_ledger_canister: args.fee_ledger_canister.as_slice().to_vec(),
            cycle_fee_e8s: args.cycle_fee_e8s,
            gas_price_buffer_bps: args.gas_price_buffer_bps,
        });
        let _ = state.wrap_evm_config.set(WrapEvmConfigStored {
            wrap_factory_address: args.wrap_factory_address,
        });
        replace_allowed_assets(state, &args.allowed_assets)
            .unwrap_or_else(|code| ic_cdk::trap(&code));
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
    validate_non_anonymous_principal(
        &args.native_ledger_canister,
        "arg.native_ledger_canister_anonymous",
    )?;
    validate_cycle_fee_e8s(args.cycle_fee_e8s)?;
    validate_gas_price_buffer_bps(args.gas_price_buffer_bps)?;
    validate_evm_address(
        args.wrap_factory_address.as_slice(),
        "arg.wrap_factory_address_invalid",
    )?;
    validate_allowed_assets(args.allowed_assets.as_slice())?;
    if args
        .allowed_assets
        .iter()
        .any(|asset| asset == &args.native_ledger_canister)
    {
        return Err("asset.native_ledger_not_wrappable".to_string());
    }
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

#[ic_cdk::update]
async fn dispatch_native_withdrawal_request(
    args: DispatchNativeWithdrawalRequestArgs,
) -> Result<DispatchUnwrapRequestOk, ApiError> {
    init_state();
    map_string_result(ensure_kasane_caller())?;
    validate_non_anonymous_principal(&args.recipient, "arg.recipient_anonymous")
        .map_err(api_invalid_argument)?;
    let request_id = request_id_or_invalid_argument(args.request_id.as_slice())?;
    if with_state(|state| state.requests.get(&request_id).is_some()) {
        return Ok(DispatchUnwrapRequestOk {
            request_id: request_id.0.to_vec(),
        });
    }
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s)
        .ok_or_else(|| api_invalid_argument("arg.amount_out_of_range".to_string()))?;
    validate_amount_bytes(amount.as_slice()).map_err(api_invalid_argument)?;
    let ledger = expected_native_ledger_canister().map_err(api_internal)?;
    let fee = fetch_icrc1_fee(ledger).await.map_err(api_rejected)?;
    let amount_u128 = nat_to_u128(&args.amount_e8s)
        .ok_or_else(|| api_invalid_argument("arg.amount_out_of_range".to_string()))?;
    let transfer_amount_e8s = native_withdraw_receive_amount(amount_u128, fee)
        .map_err(|code| api_rejected(code.to_string()))?;
    let transfer_amount = u256_from_u128(transfer_amount_e8s).to_vec();
    let normalized = NormalizedDispatchUnwrapRequest {
        request_id: request_id.0.to_vec(),
        asset_id: ledger.as_slice().to_vec(),
        amount: transfer_amount,
        recipient: args.recipient.as_slice().to_vec(),
    };
    let request_id = apply_insert_request_outcome(
        map_string_result(insert_request(normalized))?,
        schedule_worker,
    );
    Ok(DispatchUnwrapRequestOk {
        request_id: request_id.0.to_vec(),
    })
}

#[ic_cdk::query(composite = true)]
async fn quote_native_withdrawal(
    args: QuoteNativeWithdrawalArgs,
) -> Result<QuoteNativeWithdrawalOk, ApiError> {
    init_state();
    validate_non_anonymous_principal(&args.recipient, "arg.recipient_anonymous")
        .map_err(api_invalid_argument)?;
    let amount_u128 = nat_to_u128(&args.amount_e8s)
        .ok_or_else(|| api_invalid_argument("arg.amount_out_of_range".to_string()))?;
    if amount_u128 == 0 {
        return Err(api_invalid_argument("arg.amount_zero".to_string()));
    }
    let ledger = expected_native_ledger_canister().map_err(api_internal)?;
    let fee = fetch_icrc1_fee(ledger).await.map_err(api_rejected)?;
    let receive_amount = native_withdraw_receive_amount(amount_u128, fee)
        .map_err(|code| api_rejected(code.to_string()))?;
    Ok(QuoteNativeWithdrawalOk {
        native_ledger_canister: ledger,
        ledger_fee_e8s: Nat::from(fee),
        receive_amount_e8s: Nat::from(receive_amount),
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
    map_string_result(ensure_asset_allowed(normalized.asset_id.as_slice()))?;
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
    map_string_result(ensure_asset_allowed(normalized.asset_id.as_slice()))?;
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

#[ic_cdk::query(composite = true)]
async fn quote_native_deposit(
    args: QuoteNativeDepositArgs,
) -> Result<QuoteNativeDepositOk, ApiError> {
    init_state();
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s)
        .ok_or_else(|| api_invalid_argument("arg.amount_out_of_range".to_string()))?;
    if amount.iter().all(|&byte| byte == 0) {
        return Err(api_invalid_argument("arg.amount_zero".to_string()));
    }
    validate_evm_address(args.evm_recipient.as_slice(), "arg.evm_recipient_invalid")
        .map_err(api_invalid_argument)?;
    let fee_policy = map_string_result(get_fee_policy_stored())?;
    Ok(QuoteNativeDepositOk {
        charged_fee_e8s: Nat::from(fee_policy.cycle_fee_e8s),
        native_ledger_canister: expected_native_ledger_canister().map_err(api_internal)?,
        fee_ledger_canister: principal_from_bytes(fee_policy.fee_ledger_canister.as_slice())
            .map_err(api_internal)?,
    })
}

#[ic_cdk::update]
async fn submit_native_deposit(
    args: SubmitNativeDepositArgs,
) -> Result<SubmitNativeDepositOk, ApiError> {
    init_state();
    let caller = ic_cdk::api::msg_caller();
    map_string_result(validate_non_anonymous_principal(
        &caller,
        "auth.caller_anonymous",
    ))?;
    let normalized = build_submit_native_deposit(args, caller).await?;
    let request_id = normalized.request_id;
    if let Some(existing) = existing_native_deposit_response(request_id, &normalized, caller) {
        return existing;
    }
    map_string_result(reserve_pending_wrap_submission(request_id))?;
    let out = submit_native_deposit_inner(normalized, caller).await;
    release_pending_wrap_submission(request_id);
    let (request_id, fee_charge) = out?;
    Ok(SubmitNativeDepositOk {
        request_id: request_id.0.to_vec(),
        charged_fee_e8s: Nat::from(fee_charge.charged_fee_e8s),
        fee_ledger_tx_id: fee_charge.ledger_tx_id,
    })
}

fn existing_native_deposit_response(
    request_id: RequestId,
    args: &NormalizedSubmitNativeDeposit,
    caller: Principal,
) -> Option<Result<SubmitNativeDepositOk, ApiError>> {
    with_state(|state| {
        let existing = state.wrap_requests.get(&request_id)?;
        if existing.caller.as_slice() != caller.as_slice()
            || existing.amount != args.amount
            || existing.evm_recipient != args.evm_recipient
        {
            return Some(Err(api_rejected(
                "request.idempotency_mismatch".to_string(),
            )));
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

async fn submit_wrap_request_inner(
    args: NormalizedSubmitWrapRequest,
    caller: Principal,
    request_id: RequestId,
) -> Result<(RequestId, FeeCharge), ApiError> {
    let quote = quote_wrap_request_inner(args.gas_limit).await?;
    validate_quote_within_approval(&args, &quote).map_err(api_rejected)?;
    let fee_amount = u256_from_u128(quote.charged_fee_e8s);
    let fee_created_at_time = current_time_nanos();
    let fee_ledger_tx_id = attempt_icrc2_transfer_from(
        caller.as_slice().to_vec(),
        quote.fee_ledger_canister.as_slice().to_vec(),
        fee_amount.to_vec(),
        request_memo(request_id, TransferMemoKind::Fee),
        fee_created_at_time,
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
        fee_created_at_time,
    ))?;
    Ok((request_id, fee_charge))
}

async fn build_submit_native_deposit(
    args: SubmitNativeDepositArgs,
    caller: Principal,
) -> Result<NormalizedSubmitNativeDeposit, ApiError> {
    validate_native_deposit_id(args.deposit_id.as_slice()).map_err(api_invalid_argument)?;
    validate_evm_address(args.evm_recipient.as_slice(), "arg.evm_recipient_invalid")
        .map_err(api_invalid_argument)?;
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s)
        .ok_or_else(|| api_invalid_argument("arg.amount_out_of_range".to_string()))?;
    if amount.iter().all(|&byte| byte == 0) {
        return Err(api_invalid_argument("arg.amount_zero".to_string()));
    }
    let max_fee_e8s = nat_to_u128(&args.max_fee_e8s)
        .ok_or_else(|| api_invalid_argument("arg.max_fee_out_of_range".to_string()))?;
    let request_id = RequestId(derive_native_deposit_request_id(
        caller.as_slice(),
        args.deposit_id.as_slice(),
    ));
    Ok(NormalizedSubmitNativeDeposit {
        request_id,
        amount: amount.to_vec(),
        evm_recipient: args.evm_recipient,
        max_fee_e8s,
        fee_ledger_canister: args.fee_ledger_canister,
    })
}

async fn submit_native_deposit_inner(
    args: NormalizedSubmitNativeDeposit,
    caller: Principal,
) -> Result<(RequestId, FeeCharge), ApiError> {
    let fee_policy = map_string_result(get_fee_policy_stored())?;
    let fee_ledger =
        principal_from_bytes(fee_policy.fee_ledger_canister.as_slice()).map_err(api_internal)?;
    if fee_ledger != args.fee_ledger_canister {
        return Err(api_rejected("fee.ledger_changed".to_string()));
    }
    if u128::from(fee_policy.cycle_fee_e8s) > args.max_fee_e8s {
        return Err(api_rejected("fee.quote_exceeded".to_string()));
    }
    let request_id = args.request_id;
    let fee_created_at_time = current_time_nanos();
    let fee_amount = u256_from_u128(u128::from(fee_policy.cycle_fee_e8s));
    let fee_ledger_tx_id = attempt_icrc2_transfer_from(
        caller.as_slice().to_vec(),
        fee_policy.fee_ledger_canister.clone(),
        fee_amount.to_vec(),
        request_memo(request_id, TransferMemoKind::Fee),
        fee_created_at_time,
    )
    .await
    .map_err(map_fee_collection_error)
    .map_err(api_rejected)?;
    let fee_charge = FeeCharge {
        ledger_tx_id: fee_ledger_tx_id,
        charged_fee_e8s: u128::from(fee_policy.cycle_fee_e8s),
        charged_gas_price_wei: 0,
    };
    let native_ledger = expected_native_ledger_canister().map_err(api_internal)?;
    let pull_created_at_time = current_time_nanos();
    let pull = attempt_icrc2_transfer_from(
        caller.as_slice().to_vec(),
        native_ledger.as_slice().to_vec(),
        args.amount.clone(),
        request_memo(request_id, TransferMemoKind::Pull),
        pull_created_at_time,
    )
    .await;
    let mut req = WrapStoredRequest {
        caller: caller.as_slice().to_vec(),
        asset_id: native_ledger.as_slice().to_vec(),
        amount: args.amount.clone(),
        evm_recipient: args.evm_recipient.clone(),
        gas_limit: 0,
        fee_created_at_time,
        pull_created_at_time,
        withdraw_created_at_time: 0,
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
            fee_ledger_tx_id: Some(fee_charge.ledger_tx_id.clone()),
            charged_fee_e8s: Some(fee_charge.charged_fee_e8s),
            charged_gas_price_wei: Some(0),
        },
    };
    match pull {
        Ok(pull_tx_id) => {
            req.result.pull_ledger_tx_id = Some(pull_tx_id);
            match credit_native_deposit_on_gateway(request_id, args.evm_recipient, args.amount)
                .await
            {
                Ok(credit_id) => {
                    req.result.status = RequestStatus::Succeeded;
                    req.result.mint_tx_id = Some(credit_id);
                }
                Err(code) => {
                    req.result.status = RequestStatus::Failed;
                    req.result.error_code = Some(code);
                    req.result.mint_failed_recoverable = true;
                }
            }
        }
        Err(code) => {
            req.result.status = RequestStatus::Failed;
            req.result.error_code = Some(code);
        }
    }
    with_state_mut(|state| {
        if let Some(existing) = state.wrap_requests.get(&request_id) {
            if existing.asset_id != req.asset_id
                || existing.amount != req.amount
                || existing.evm_recipient != req.evm_recipient
            {
                return Err("request.idempotency_mismatch".to_string());
            }
            return Ok(());
        }
        state.wrap_requests.insert(request_id, req);
        Ok(())
    })
    .map_err(api_rejected)?;
    Ok((request_id, fee_charge))
}

fn api_error_code(err: ApiError) -> String {
    match err {
        ApiError::InvalidArgument(detail)
        | ApiError::Rejected(detail)
        | ApiError::Internal(detail) => detail.code,
    }
}

fn validate_quote_within_approval(
    args: &NormalizedSubmitWrapRequest,
    quote: &WrapQuote,
) -> Result<(), String> {
    if quote.fee_ledger_canister != args.fee_ledger_canister {
        return Err("fee.ledger_changed".to_string());
    }
    if quote.charged_fee_e8s > args.max_fee_e8s {
        return Err("fee.quote_exceeded".to_string());
    }
    if quote.charged_gas_price_wei > args.quoted_gas_price_wei {
        return Err("fee.gas_price_exceeded".to_string());
    }
    Ok(())
}

#[ic_cdk::update]
async fn recover_failed_wrap(args: RecoverFailedWrapArgs) -> Result<RequestOverview, ApiError> {
    init_state();
    let request_id = request_id_or_invalid_argument(args.request_id.as_slice())?;
    let caller = ic_cdk::api::msg_caller();
    let (asset_id, amount) = map_string_result(reserve_failed_wrap_withdraw(request_id, caller))?;

    let transfer = attempt_icrc1_transfer(
        asset_id,
        amount,
        caller.as_slice().to_vec(),
        request_memo(request_id, TransferMemoKind::Withdraw),
        with_state(|state| {
            state
                .wrap_requests
                .get(&request_id)
                .map(|req| req.withdraw_created_at_time)
                .unwrap_or(0)
        }),
    )
    .await;
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
    let transfer = attempt_icrc1_transfer(
        asset_id,
        amount,
        recipient,
        request_memo(request_id, TransferMemoKind::Unwrap),
        with_state(|state| {
            state
                .requests
                .get(&request_id)
                .map(|req| req.created_at_time)
                .unwrap_or(0)
        }),
    )
    .await;
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

#[ic_cdk::update]
async fn retry_native_withdrawal(args: RetryRequestArgs) -> Result<RequestOverview, ApiError> {
    retry_request(args).await
}

#[ic_cdk::query]
fn get_native_deposit_result(request_id: Vec<u8>) -> Option<RequestOverview> {
    get_request(request_id)
}

#[ic_cdk::update]
async fn retry_native_deposit(args: RetryRequestArgs) -> Result<RequestOverview, ApiError> {
    init_state();
    let request_id = request_id_or_invalid_argument(args.request_id.as_slice())?;
    let caller = ic_cdk::api::msg_caller();
    let (evm_recipient, amount) = with_state(|state| {
        let req = state.wrap_requests.get(&request_id)?;
        if req.caller.as_slice() != caller.as_slice()
            || req.result.status != RequestStatus::Failed
            || !req.result.mint_failed_recoverable
            || req.result.pull_ledger_tx_id.is_none()
            || req.result.mint_tx_id.is_some()
        {
            return None;
        }
        Some((req.evm_recipient.clone(), req.amount.clone()))
    })
    .ok_or_else(|| api_rejected("native_deposit.retry_invalid_state".to_string()))?;
    match credit_native_deposit_on_gateway(request_id, evm_recipient, amount).await {
        Ok(credit_id) => {
            with_state_mut(|state| {
                if let Some(mut req) = state.wrap_requests.get(&request_id) {
                    req.result.status = RequestStatus::Succeeded;
                    req.result.mint_tx_id = Some(credit_id);
                    req.result.error_code = None;
                    req.result.mint_failed_recoverable = false;
                    state.wrap_requests.insert(request_id, req);
                }
            });
            request_overview_or_internal(request_id)
        }
        Err(code) => {
            with_state_mut(|state| {
                if let Some(mut req) = state.wrap_requests.get(&request_id) {
                    req.result.error_code = Some(code.clone());
                    state.wrap_requests.insert(request_id, req);
                }
            });
            Err(api_rejected(code))
        }
    }
}

fn insert_request(args: NormalizedDispatchUnwrapRequest) -> Result<InsertRequestOutcome, String> {
    let request_id = to_request_id(args.request_id.as_slice())?;
    validate_principal_bytes(args.asset_id.as_slice())?;
    validate_principal_bytes(args.recipient.as_slice())?;
    let created_at_time = current_time_nanos();
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
        // kasane から受け取った burn 済み unwrap liability は、
        // delist 後でも登録して払い出し完了まで進める。
        state.requests.insert(
            request_id,
            StoredRequest {
                asset_id: args.asset_id,
                amount: args.amount,
                recipient: args.recipient,
                created_at_time,
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
) -> Result<RetryTransferReservation, String> {
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
        let _ = state.queue_meta.set(meta);
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
            let _ = state.queue_meta.set(meta);
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
    fee_created_at_time: u64,
) -> Result<RequestId, String> {
    let pull_created_at_time = current_time_nanos();
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
                fee_created_at_time,
                pull_created_at_time,
                withdraw_created_at_time: 0,
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
        asset_id: args.asset_id.as_slice().to_vec(),
        gas_limit: args.gas_limit,
    })
}

fn normalize_submit_wrap_args(
    args: SubmitWrapRequestArgs,
) -> Result<NormalizedSubmitWrapArgsParts, String> {
    validate_principal_bytes(args.asset_id.as_slice())?;
    validate_non_anonymous_principal(
        &args.fee_ledger_canister,
        "arg.fee_ledger_canister_anonymous",
    )?;
    let amount = nat_to_fixed_be::<32>(&args.amount_e8s)
        .ok_or_else(|| "arg.amount_out_of_range".to_string())?
        .to_vec();
    let max_fee_e8s =
        nat_to_u128(&args.max_fee_e8s).ok_or_else(|| "arg.max_fee_out_of_range".to_string())?;
    let quoted_gas_price_wei = nat_to_u128(&args.quoted_gas_price_wei)
        .ok_or_else(|| "arg.quoted_gas_price_out_of_range".to_string())?;
    validate_amount_bytes(amount.as_slice())?;
    validate_evm_address(args.evm_recipient.as_slice(), "arg.evm_recipient_invalid")?;
    if args.gas_limit == 0 {
        return Err("arg.gas_limit_invalid".to_string());
    }
    if max_fee_e8s == 0 {
        return Err("arg.max_fee_invalid".to_string());
    }
    if quoted_gas_price_wei == 0 {
        return Err("arg.quoted_gas_price_invalid".to_string());
    }
    Ok((
        args.asset_id.as_slice().to_vec(),
        amount,
        args.evm_recipient,
        args.evm_nonce,
        args.gas_limit,
        max_fee_e8s,
        quoted_gas_price_wei,
        args.fee_ledger_canister,
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

fn validate_allowed_assets(assets: &[Principal]) -> Result<(), String> {
    for asset in assets {
        validate_non_anonymous_principal(asset, "arg.allowed_asset_anonymous")?;
    }
    Ok(())
}

fn replace_allowed_assets(state: &mut StableState, assets: &[Principal]) -> Result<(), String> {
    validate_allowed_assets(assets)?;
    let native = state.native_ledger_canister.get().clone();
    if assets
        .iter()
        .any(|asset| asset.as_slice() == native.as_slice())
    {
        return Err("asset.native_ledger_not_wrappable".to_string());
    }
    let keys: Vec<_> = state
        .allowed_assets
        .range(..)
        .map(|entry| entry.key().clone())
        .collect();
    for key in keys {
        state.allowed_assets.remove(&key);
    }
    for asset in assets {
        state.allowed_assets.insert(asset.as_slice().to_vec(), 1);
    }
    Ok(())
}

fn ensure_asset_allowed(asset_id: &[u8]) -> Result<(), String> {
    validate_principal_bytes(asset_id)?;
    let allowed = with_state(|state| {
        if state.native_ledger_canister.get().as_slice() == asset_id {
            return false;
        }
        state.allowed_assets.contains_key(&asset_id.to_vec())
    });
    if allowed {
        Ok(())
    } else {
        Err("asset.not_allowed".to_string())
    }
}

fn allowed_assets_view() -> Result<Vec<Principal>, String> {
    with_state(|state| {
        state
            .allowed_assets
            .range(..)
            .map(|entry| principal_from_bytes(entry.key().as_slice()))
            .collect()
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
    let padded_len = asset_id.len().div_ceil(32) * 32;
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
    let (
        asset_id,
        amount,
        evm_recipient,
        evm_nonce,
        gas_limit,
        max_fee_e8s,
        quoted_gas_price_wei,
        fee_ledger_canister,
    ) = normalize_submit_wrap_args(args).map_err(api_invalid_argument)?;
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
        max_fee_e8s,
        quoted_gas_price_wei,
        fee_ledger_canister,
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

fn derive_native_deposit_request_id(from_owner: &[u8], deposit_id: &[u8]) -> [u8; 32] {
    let mut keccak = Keccak::v256();
    keccak.update(b"kasane.native.deposit.v2");
    hash_len_prefixed(&mut keccak, from_owner);
    hash_len_prefixed(&mut keccak, deposit_id);
    let mut out = [0u8; 32];
    keccak.finalize(&mut out);
    out
}

fn validate_native_deposit_id(deposit_id: &[u8]) -> Result<(), String> {
    if deposit_id.len() != 32 {
        return Err("arg.deposit_id_invalid".to_string());
    }
    Ok(())
}

fn request_memo(request_id: RequestId, kind: TransferMemoKind) -> Vec<u8> {
    let kind_byte = match kind {
        TransferMemoKind::Unwrap => 1,
        TransferMemoKind::Fee => 2,
        TransferMemoKind::Pull => 3,
        TransferMemoKind::Withdraw => 4,
    };
    // Ledger memo must stay within 32 bytes while still distinguishing transfer kind.
    let mut keccak = Keccak::v256();
    keccak.update(b"kasane.wrap.memo.v1");
    keccak.update(&[kind_byte]);
    keccak.update(&request_id.0);
    let mut memo = vec![0u8; 32];
    keccak.finalize(&mut memo);
    memo
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
        let _ = state.wrap_queue_meta.set(meta);
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
            let _ = state.wrap_queue_meta.set(meta);
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

#[ic_cdk::query]
fn get_allowed_assets() -> Result<Vec<Principal>, String> {
    init_state();
    allowed_assets_view()
}

#[ic_cdk::query]
fn icrc10_supported_standards() -> Vec<icrc21::StandardRecord> {
    init_state();
    icrc21::supported_standards()
}

#[ic_cdk::update]
async fn icrc21_canister_call_consent_message(
    request: icrc21::Icrc21ConsentMessageRequest,
) -> icrc21::Icrc21ConsentMessageResponse {
    icrc21::consent_message(request).await
}

fn recover_request_state_after_upgrade(_now: u64) -> bool {
    with_state_mut(|state| {
        let mut queued_ids = BTreeSet::new();
        for entry in state.queue.range(..) {
            queued_ids.insert(entry.value());
        }

        let mut candidates = Vec::new();
        for entry in state.requests.range(..) {
            let request_id = *entry.key();
            let mut req = entry.value().clone();
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
        let _ = state.queue_meta.set(meta);
        !state.queue_meta.get().is_empty()
    })
}

fn recover_wrap_request_state_after_upgrade(_now: u64) -> bool {
    with_state_mut(|state| {
        let mut queued_ids = BTreeSet::new();
        for entry in state.wrap_queue.range(..) {
            queued_ids.insert(entry.value());
        }

        let mut in_progress_to_clear = Vec::new();
        let mut candidates = Vec::new();
        for entry in state.wrap_requests.range(..) {
            let request_id = *entry.key();
            let mut req = entry.value().clone();
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
        let _ = state.wrap_queue_meta.set(meta);
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
    validate_cycle_fee_e8s(args.cycle_fee_e8s)?;
    validate_gas_price_buffer_bps(args.gas_price_buffer_bps)?;
    with_state_mut(|state| {
        let _ = state.fee_policy.set(FeePolicyStored {
            fee_ledger_canister: args.fee_ledger_canister.as_slice().to_vec(),
            cycle_fee_e8s: args.cycle_fee_e8s,
            gas_price_buffer_bps: args.gas_price_buffer_bps,
        });
    });
    Ok(())
}

#[ic_cdk::update]
fn set_allowed_assets(args: Vec<Principal>) -> Result<(), String> {
    init_state();
    ensure_controller_caller()?;
    with_state_mut(|state| replace_allowed_assets(state, args.as_slice()))
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
        if req.withdraw_created_at_time == 0 {
            req.withdraw_created_at_time = current_time_nanos();
        }
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
                    req.created_at_time,
                )
            })
        });
        let Some((asset_id, amount, recipient, created_at_time)) = req else {
            continue;
        };
        mark_request_running(request_id);
        // allowlist is only for new intake and pre-pull wrap execution.
        // Accepted unwrap liabilities must remain payable after delist.
        let result = attempt_icrc1_transfer(
            asset_id,
            amount,
            recipient,
            request_memo(request_id, TransferMemoKind::Unwrap),
            created_at_time,
        )
        .await;
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
                    req.pull_created_at_time,
                    req.result.charged_gas_price_wei.unwrap_or(0),
                )
            })
        });
        let Some((
            caller,
            asset_id,
            amount,
            evm_recipient,
            gas_limit,
            pull_created_at_time,
            charged_gas_price_wei,
        )) = req
        else {
            continue;
        };
        mark_wrap_request_running(request_id);

        // allowlist は submit 時点でのみ適用する。
        // fee 徴収後に受理済み request を delist で失敗させない。
        let pull = attempt_icrc2_transfer_from(
            caller,
            asset_id.clone(),
            amount.clone(),
            request_memo(request_id, TransferMemoKind::Pull),
            pull_created_at_time,
        )
        .await;
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
    memo: Vec<u8>,
    created_at_time: u64,
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
        memo: Some(memo),
        created_at_time: Some(created_at_time),
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
    memo: Vec<u8>,
    created_at_time: u64,
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
        memo: Some(memo),
        created_at_time: Some(created_at_time),
    };
    let call_result = ic_cdk::call::Call::unbounded_wait(ledger, "icrc2_transfer_from")
        .with_arg(arg)
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<Nat, Icrc2TransferFromError>,)>() {
            Ok((result,)) => match result {
                Ok(block_index) => Ok(nat_to_be_bytes(&block_index)),
                Err(Icrc2TransferFromError::Duplicate { duplicate_of }) => {
                    Ok(nat_to_be_bytes(&duplicate_of))
                }
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

async fn credit_native_deposit_on_gateway(
    request_id: RequestId,
    evm_recipient: Vec<u8>,
    amount_e8s: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let gateway = expected_evm_gateway_canister()?;
    let amount = nat_from_32_be(amount_e8s.as_slice())?;
    let amount_wei = nat_mul_u128(&amount, WEI_PER_E8S);
    let call_result = ic_cdk::call::Call::unbounded_wait(gateway, "credit_native_deposit")
        .with_arg((request_id.0.to_vec(), evm_recipient, amount_wei))
        .await;
    match call_result {
        Ok(resp) => match resp.candid_tuple::<(Result<(), ApiError>,)>() {
            Ok((Ok(()),)) => Ok(request_id.0.to_vec()),
            Ok((Err(err),)) => Err(format!("evm_gateway.credit_failed:{}", api_error_code(err))),
            Err(err) => Err(format!("evm_gateway.credit_decode_failed:{err}")),
        },
        Err(err) => Err(format!("evm_gateway.credit_call_failed:{err}")),
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
        Err(Icrc1TransferError::Duplicate { duplicate_of }) => Ok(nat_to_be_bytes(&duplicate_of)),
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

fn nat_mul_u128(value: &Nat, multiplier: u128) -> Nat {
    Nat(&value.0 * BigUint::from(multiplier))
}

fn native_withdraw_receive_amount(
    amount_e8s: u128,
    ledger_fee_e8s: u128,
) -> Result<u128, &'static str> {
    if amount_e8s <= ledger_fee_e8s {
        return Err("native_withdraw.amount_not_above_fee");
    }
    Ok(amount_e8s - ledger_fee_e8s)
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

fn validate_cycle_fee_e8s(value: u64) -> Result<(), String> {
    if value > MAX_CYCLE_FEE_E8S {
        return Err("arg.cycle_fee_e8s_out_of_range".to_string());
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

fn expected_native_ledger_canister() -> Result<Principal, String> {
    let expected = with_state(|state| state.native_ledger_canister.get().clone());
    if expected.is_empty() {
        return Err("config.native_ledger_missing".to_string());
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
    out.push(2u8);
    write_u8_len_bytes(&mut out, &value.asset_id)?;
    out.extend_from_slice(&value.amount);
    write_u8_len_bytes(&mut out, &value.recipient)?;
    out.extend_from_slice(&value.created_at_time.to_be_bytes());
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
    if version != 2 {
        return None;
    }
    let asset_id = read_u8_len_bytes(bytes, &mut offset, PRINCIPAL_MAX_BYTES)?;
    let amount_end = offset.checked_add(AMOUNT_BYTES)?;
    let amount = bytes.get(offset..amount_end)?.to_vec();
    offset = amount_end;
    let recipient = read_u8_len_bytes(bytes, &mut offset, PRINCIPAL_MAX_BYTES)?;
    let end = offset.checked_add(8)?;
    let raw = bytes.get(offset..end)?;
    offset = end;
    let created_at_time = u64::from_be_bytes(raw.try_into().ok()?);
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
        created_at_time,
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
mod tests;
