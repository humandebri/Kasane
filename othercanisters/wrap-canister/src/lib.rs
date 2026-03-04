//! where: wrap/vault canister
//! what: unwrap + wrap request queue workers
//! why: split asset execution from kasane core

use candid::{CandidType, Deserialize, Nat, Principal};
use ic_stable_structures::memory_manager::{MemoryId, MemoryManager, VirtualMemory};
use ic_stable_structures::storable::Bound;
use ic_stable_structures::{DefaultMemoryImpl, StableBTreeMap, StableCell, Storable};
use num_bigint::BigUint;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use tiny_keccak::{Hasher, Keccak};

const MEM_KASANE_CANISTER: MemoryId = MemoryId::new(0);
const MEM_REQUESTS: MemoryId = MemoryId::new(1);
const MEM_QUEUE: MemoryId = MemoryId::new(2);
const MEM_QUEUE_META: MemoryId = MemoryId::new(3);
const MEM_EVM_GATEWAY_CANISTER: MemoryId = MemoryId::new(4);
const MEM_WRAP_REQUESTS: MemoryId = MemoryId::new(5);
const MEM_WRAP_QUEUE: MemoryId = MemoryId::new(6);
const MEM_WRAP_QUEUE_META: MemoryId = MemoryId::new(7);
const MEM_EVM_WRAP_FACTORY: MemoryId = MemoryId::new(8);
const PRINCIPAL_MAX_BYTES: usize = 29;
const AMOUNT_BYTES: usize = 32;
const EVM_ADDRESS_BYTES: usize = 20;
const MAX_LEDGER_TX_ID_BYTES: usize = 128;
const MAX_ERROR_CODE_BYTES: usize = 192;
const STORED_REQUEST_MAX_BYTES: u32 = 448;
const WRAP_STORED_REQUEST_MAX_BYTES: u32 = 768;
const DEFAULT_MINT_GAS_LIMIT: u64 = 300_000;

type Memory = VirtualMemory<DefaultMemoryImpl>;

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct InitArgs {
    pub kasane_canister: Principal,
    pub evm_gateway_canister: Principal,
    pub evm_wrap_factory: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SubmitUnwrapRequestArgs {
    pub request_id: Vec<u8>,
    pub asset_id: Vec<u8>,
    pub amount: Vec<u8>,
    pub recipient: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SubmitUnwrapRequestOk {
    pub request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SubmitWrapRequestArgs {
    pub request_id: Vec<u8>,
    pub asset_id: Vec<u8>,
    pub amount: Vec<u8>,
    pub from_owner: Vec<u8>,
    pub evm_recipient: Vec<u8>,
    pub evm_nonce: u64,
    pub gas_limit: u64,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct SubmitWrapRequestOk {
    pub request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WithdrawFailedWrapArgs {
    pub request_id: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize)]
pub struct WithdrawFailedWrapOk {
    pub request_id: Vec<u8>,
    pub ledger_tx_id: Vec<u8>,
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
    pub mint_failed_recoverable: bool,
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
    evm_nonce: u64,
    gas_limit: u64,
    result: WrapRequestResult,
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

struct StableState {
    kasane_canister: StableCell<Vec<u8>, Memory>,
    evm_gateway_canister: StableCell<Vec<u8>, Memory>,
    evm_wrap_factory: StableCell<Vec<u8>, Memory>,
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
        let evm_wrap_factory = with_memory(MEM_EVM_WRAP_FACTORY, |memory| {
            StableCell::init(memory, Vec::<u8>::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell init failed: evm_wrap_factory"))
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
            evm_wrap_factory,
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
    if let Err(code) =
        validate_non_anonymous_principal(&args.kasane_canister, "arg.kasane_canister_anonymous")
    {
        ic_cdk::trap(&code);
    }
    if let Err(code) = validate_non_anonymous_principal(
        &args.evm_gateway_canister,
        "arg.evm_gateway_canister_anonymous",
    ) {
        ic_cdk::trap(&code);
    }
    if let Err(code) = validate_evm_address(
        args.evm_wrap_factory.as_slice(),
        "arg.evm_wrap_factory_invalid",
    ) {
        ic_cdk::trap(&code);
    }
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
            .evm_wrap_factory
            .set(args.evm_wrap_factory)
            .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: evm_wrap_factory"));
    });
}

#[ic_cdk::post_upgrade]
fn post_upgrade() {
    init_state();
    if with_state(|state| !state.queue_meta.get().is_empty()) {
        schedule_worker();
    }
    if with_state(|state| !state.wrap_queue_meta.get().is_empty()) {
        schedule_wrap_worker();
    }
}

#[ic_cdk::update]
fn submit_unwrap_request(args: SubmitUnwrapRequestArgs) -> Result<SubmitUnwrapRequestOk, String> {
    init_state();
    ensure_kasane_caller()?;
    let request_id = insert_request(args)?;
    enqueue_request(request_id);
    schedule_worker();
    Ok(SubmitUnwrapRequestOk {
        request_id: request_id.0.to_vec(),
    })
}

#[ic_cdk::update]
fn submit_wrap_request(args: SubmitWrapRequestArgs) -> Result<SubmitWrapRequestOk, String> {
    init_state();
    ensure_kasane_caller()?;
    let request_id = insert_wrap_request(args)?;
    enqueue_wrap_request(request_id);
    schedule_wrap_worker();
    Ok(SubmitWrapRequestOk {
        request_id: request_id.0.to_vec(),
    })
}

#[ic_cdk::update]
async fn withdraw_failed_wrap(
    args: WithdrawFailedWrapArgs,
) -> Result<WithdrawFailedWrapOk, String> {
    init_state();
    let request_id = to_request_id(args.request_id.as_slice())?;
    let caller = ic_cdk::api::msg_caller();
    let (asset_id, amount) = with_state(|state| {
        let req = state.wrap_requests.get(&request_id);
        match req {
            Some(req) => {
                validate_withdraw_request(&req, caller)?;
                Ok((req.asset_id.clone(), req.amount.clone()))
            }
            None => Err("withdraw.invalid_state".to_string()),
        }
    })?;

    let transfer = attempt_icrc1_transfer(asset_id, amount, caller.as_slice().to_vec()).await;
    match transfer {
        Ok(tx_id) => {
            with_state_mut(|state| {
                if let Some(mut req) = state.wrap_requests.get(&request_id) {
                    req.result.withdrawn = true;
                    req.result.withdraw_ledger_tx_id = Some(tx_id.clone());
                    req.result.withdraw_error_code = None;
                    req.result.mint_failed_recoverable = false;
                    state.wrap_requests.insert(request_id, req);
                }
            });
            Ok(WithdrawFailedWrapOk {
                request_id: request_id.0.to_vec(),
                ledger_tx_id: tx_id,
            })
        }
        Err(code) => {
            let withdraw_code = to_withdraw_error_code(&code);
            with_state_mut(|state| {
                if let Some(mut req) = state.wrap_requests.get(&request_id) {
                    req.result.withdraw_error_code = Some(withdraw_code.clone());
                    state.wrap_requests.insert(request_id, req);
                }
            });
            Err(withdraw_code)
        }
    }
}

fn insert_request(args: SubmitUnwrapRequestArgs) -> Result<RequestId, String> {
    let request_id = to_request_id(args.request_id.as_slice())?;
    validate_principal_bytes(args.asset_id.as_slice())?;
    validate_principal_bytes(args.recipient.as_slice())?;
    let inserted = with_state_mut(|state| {
        if state.requests.contains_key(&request_id) {
            return false;
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
                },
            },
        );
        true
    });
    if !inserted {
        return Err("request.duplicate".to_string());
    }
    Ok(request_id)
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

fn insert_wrap_request(args: SubmitWrapRequestArgs) -> Result<RequestId, String> {
    let request_id = to_request_id(args.request_id.as_slice())?;
    validate_principal_bytes(args.asset_id.as_slice())?;
    validate_amount_bytes(args.amount.as_slice())?;
    let from_owner = principal_from_bytes(args.from_owner.as_slice())?;
    if from_owner == Principal::anonymous() {
        return Err("arg.from_owner_anonymous".to_string());
    }
    validate_evm_address(args.evm_recipient.as_slice(), "arg.evm_recipient_invalid")?;
    let expected_request_id = derive_wrap_request_id(
        from_owner.as_slice(),
        args.asset_id.as_slice(),
        args.amount.as_slice(),
        args.evm_recipient.as_slice(),
        args.evm_nonce,
        args.gas_limit,
    );
    if request_id.0 != expected_request_id {
        return Err("arg.request_id_mismatch".to_string());
    }
    let inserted = with_state_mut(|state| {
        if state.wrap_requests.contains_key(&request_id) {
            return false;
        }
        state.wrap_requests.insert(
            request_id,
            WrapStoredRequest {
                caller: from_owner.as_slice().to_vec(),
                asset_id: args.asset_id,
                amount: args.amount,
                evm_recipient: args.evm_recipient,
                evm_nonce: args.evm_nonce,
                gas_limit: args.gas_limit,
                result: WrapRequestResult {
                    status: RequestStatus::Queued,
                    pull_ledger_tx_id: None,
                    mint_tx_id: None,
                    error_code: None,
                    withdrawn: false,
                    withdraw_ledger_tx_id: None,
                    withdraw_error_code: None,
                    mint_failed_recoverable: false,
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
fn get_request_status(request_id: Vec<u8>) -> Option<RequestStatus> {
    init_state();
    let request_id = to_request_id(request_id.as_slice()).ok()?;
    with_state(|state| state.requests.get(&request_id).map(|v| v.result.status))
}

#[ic_cdk::query]
fn get_request_result(request_id: Vec<u8>) -> Option<RequestResult> {
    init_state();
    let request_id = to_request_id(request_id.as_slice()).ok()?;
    with_state(|state| state.requests.get(&request_id).map(|v| v.result.clone()))
}

#[ic_cdk::query]
fn get_wrap_request_status(request_id: Vec<u8>) -> Option<RequestStatus> {
    init_state();
    let request_id = to_request_id(request_id.as_slice()).ok()?;
    with_state(|state| {
        state
            .wrap_requests
            .get(&request_id)
            .map(|v| v.result.status)
    })
}

#[ic_cdk::query]
fn get_wrap_request_result(request_id: Vec<u8>) -> Option<WrapRequestResult> {
    init_state();
    let request_id = to_request_id(request_id.as_slice()).ok()?;
    with_state(|state| {
        state
            .wrap_requests
            .get(&request_id)
            .map(|v| v.result.clone())
    })
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

fn validate_withdraw_request(req: &WrapStoredRequest, caller: Principal) -> Result<(), String> {
    if req.caller.as_slice() != caller.as_slice() {
        return Err("withdraw.not_request_owner".to_string());
    }
    if req.result.withdrawn {
        return Err("withdraw.already_withdrawn".to_string());
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
                match result {
                    Ok(tx_id) => {
                        req.result.status = RequestStatus::Succeeded;
                        req.result.ledger_tx_id = Some(tx_id);
                        req.result.error_code = None;
                    }
                    Err(code) => {
                        req.result.status = RequestStatus::Failed;
                        req.result.ledger_tx_id = None;
                        req.result.error_code = Some(code);
                    }
                }
                state.requests.insert(request_id, req);
            }
        });
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
                    req.evm_nonce,
                    req.gas_limit,
                )
            })
        });
        let Some((caller, asset_id, amount, evm_recipient, evm_nonce, gas_limit)) = req else {
            continue;
        };
        mark_wrap_request_running(request_id);

        let pull = attempt_icrc2_transfer_from(caller, asset_id.clone(), amount.clone()).await;
        let outcome = match pull {
            Ok(pull_tx_id) => {
                let mint = submit_mint_tx(
                    asset_id,
                    evm_recipient,
                    amount,
                    evm_nonce,
                    if gas_limit == 0 {
                        DEFAULT_MINT_GAS_LIMIT
                    } else {
                        gas_limit
                    },
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
struct SubmitIcTxArgsDto {
    to: Option<Vec<u8>>,
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

async fn submit_mint_tx(
    asset_id: Vec<u8>,
    evm_recipient: Vec<u8>,
    amount: Vec<u8>,
    nonce: u64,
    gas_limit: u64,
) -> Result<Vec<u8>, String> {
    validate_principal_bytes(asset_id.as_slice())?;
    validate_evm_address(evm_recipient.as_slice(), "arg.evm_recipient_invalid")?;
    validate_amount_bytes(amount.as_slice())?;
    let gateway = expected_evm_gateway_canister()?;
    let factory = expected_evm_wrap_factory()?;
    let data = encode_factory_mint_for_asset_call_data(
        asset_id.as_slice(),
        evm_recipient.as_slice(),
        amount.as_slice(),
    )?;
    let args = SubmitIcTxArgsDto {
        to: Some(factory),
        value: Nat::from(0u8),
        gas_limit,
        nonce,
        max_fee_per_gas: Nat::from(0u8),
        max_priority_fee_per_gas: Nat::from(0u8),
        data,
    };

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

fn mark_request_running(request_id: RequestId) {
    with_state_mut(|state| {
        if let Some(mut req) = state.requests.get(&request_id) {
            req.result.status = RequestStatus::Running;
            req.result.ledger_tx_id = None;
            req.result.error_code = None;
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
    let expected = with_state(|state| state.evm_wrap_factory.get().clone());
    if expected.is_empty() {
        return Err("config.evm_wrap_factory_missing".to_string());
    }
    validate_evm_address(expected.as_slice(), "config.evm_wrap_factory_invalid")?;
    Ok(expected)
}

fn encode_factory_mint_for_asset_call_data(
    asset_id: &[u8],
    recipient: &[u8],
    amount: &[u8],
) -> Result<Vec<u8>, String> {
    validate_principal_bytes(asset_id)?;
    validate_evm_address(recipient, "arg.evm_recipient_invalid")?;
    validate_amount_bytes(amount)?;
    let mut data = Vec::with_capacity(4 + 32 * 4 + 64);
    data.extend_from_slice(&factory_mint_for_asset_selector());
    data.extend_from_slice(&u256_from_u64(96));
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

fn factory_mint_for_asset_selector() -> [u8; 4] {
    let mut keccak = Keccak::v256();
    keccak.update(b"mintForAsset(bytes,address,uint256)");
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
    if version != 1 {
        return None;
    }
    offset += 1;
    let asset_id = read_u8_len_bytes(bytes, &mut offset, PRINCIPAL_MAX_BYTES)?;
    let amount_end = offset.checked_add(AMOUNT_BYTES)?;
    let amount = bytes.get(offset..amount_end)?.to_vec();
    offset = amount_end;
    let recipient = read_u8_len_bytes(bytes, &mut offset, PRINCIPAL_MAX_BYTES)?;
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
        decode_stored_request, dequeue_request, derive_wrap_request_id,
        encode_factory_mint_for_asset_call_data, encode_stored_request, enqueue_request,
        init_state, insert_request, insert_wrap_request, is_withdrawable, map_transfer_reply,
        mark_request_running, mark_wrap_request_running, nat_from_32_be, nat_to_be_bytes,
        on_worker_queue_drain, on_wrap_worker_queue_drain, principal_from_bytes, schedule_worker,
        schedule_wrap_worker, submit_error_to_code, to_request_id, to_withdraw_error_code,
        transfer_error_to_code, transfer_from_error_to_code, u256_from_u64,
        validate_non_anonymous_principal, validate_withdraw_request, with_state, with_state_mut,
        Icrc1TransferError, Icrc2TransferFromError, QueueMeta, RequestResult, RequestStatus,
        StoredRequest, SubmitTxError, SubmitUnwrapRequestArgs, SubmitWrapRequestArgs,
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
                .evm_wrap_factory
                .set(Vec::new())
                .unwrap_or_else(|_| ic_cdk::trap("stable cell set failed: evm_wrap_factory"));
        });
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
    }

    #[test]
    fn insert_request_rejects_duplicate() {
        reset_state();
        let args = SubmitUnwrapRequestArgs {
            request_id: vec![1u8; 32],
            asset_id: vec![2u8; 29],
            amount: vec![0u8; 32],
            recipient: vec![3u8; 29],
        };
        let first = insert_request(args.clone()).expect("first should pass");
        assert_eq!(first, to_request_id(&[1u8; 32]).expect("id"));
        let err = insert_request(args).expect_err("second should fail");
        assert_eq!(err, "request.duplicate");
        let status = with_state(|state| {
            state
                .requests
                .get(&to_request_id(&[1u8; 32]).expect("id"))
                .map(|r| r.result.status)
        });
        assert_eq!(status, Some(RequestStatus::Queued));
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
            result: RequestResult {
                status: RequestStatus::Queued,
                ledger_tx_id: None,
                error_code: None,
            },
        };
        assert!(encode_stored_request(&req).is_none());
        assert!(decode_stored_request(&[0xFF]).is_none());
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
        insert_request(SubmitUnwrapRequestArgs {
            request_id: request_id.0.to_vec(),
            asset_id: vec![2u8; 29],
            amount: vec![3u8; 32],
            recipient: vec![4u8; 29],
        })
        .expect("insert");
        mark_request_running(request_id);
        let status = with_state(|state| state.requests.get(&request_id).map(|v| v.result.status));
        assert_eq!(status, Some(RequestStatus::Running));
    }

    #[test]
    fn nat_to_be_bytes_preserves_high_bit_width() {
        let value = Nat(BigUint::from(1u8) << 200usize);
        let encoded = nat_to_be_bytes(&value);
        assert!(encoded.len() > 16);
        assert_eq!(encoded.first().copied(), Some(1u8));
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
        let from_owner = vec![6u8; 29];
        let asset_id = vec![2u8; 29];
        let amount = vec![0u8; 32];
        let evm_recipient = vec![4u8; 20];
        let request_id = derive_wrap_request_id(
            from_owner.as_slice(),
            asset_id.as_slice(),
            amount.as_slice(),
            evm_recipient.as_slice(),
            1,
            200_000,
        );
        let args = SubmitWrapRequestArgs {
            request_id: request_id.to_vec(),
            asset_id,
            amount,
            from_owner,
            evm_recipient,
            evm_nonce: 1,
            gas_limit: 200_000,
        };
        insert_wrap_request(args.clone()).expect("first should pass");
        let err = insert_wrap_request(args).expect_err("second should fail");
        assert_eq!(err, "wrap.request.duplicate");
    }

    #[test]
    fn mark_wrap_request_running_sets_running_status() {
        reset_state();
        let from_owner = vec![6u8; 29];
        let asset_id = vec![2u8; 29];
        let amount = vec![3u8; 32];
        let evm_recipient = vec![5u8; 20];
        let request_id_raw = derive_wrap_request_id(
            from_owner.as_slice(),
            asset_id.as_slice(),
            amount.as_slice(),
            evm_recipient.as_slice(),
            1,
            300_000,
        );
        let request_id = to_request_id(&request_id_raw).expect("id");
        insert_wrap_request(SubmitWrapRequestArgs {
            request_id: request_id_raw.to_vec(),
            asset_id,
            amount,
            from_owner,
            evm_recipient,
            evm_nonce: 1,
            gas_limit: 300_000,
        })
        .expect("insert");
        mark_wrap_request_running(request_id);
        let status = with_state(|state| {
            state
                .wrap_requests
                .get(&request_id)
                .map(|v| v.result.status)
        });
        assert_eq!(status, Some(RequestStatus::Running));
    }

    #[test]
    fn wrap_insert_request_rejects_anonymous_from_owner() {
        reset_state();
        let err = insert_wrap_request(SubmitWrapRequestArgs {
            request_id: vec![8u8; 32],
            asset_id: vec![2u8; 29],
            amount: vec![3u8; 32],
            from_owner: Principal::anonymous().as_slice().to_vec(),
            evm_recipient: vec![5u8; 20],
            evm_nonce: 1,
            gas_limit: 300_000,
        })
        .expect_err("anonymous from_owner must fail");
        assert_eq!(err, "arg.from_owner_anonymous");
    }

    #[test]
    fn wrap_insert_request_rejects_request_id_mismatch() {
        reset_state();
        let err = insert_wrap_request(SubmitWrapRequestArgs {
            request_id: vec![8u8; 32],
            asset_id: vec![2u8; 29],
            amount: vec![3u8; 32],
            from_owner: vec![6u8; 29],
            evm_recipient: vec![5u8; 20],
            evm_nonce: 1,
            gas_limit: 300_000,
        })
        .expect_err("mismatch must fail");
        assert_eq!(err, "arg.request_id_mismatch");
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
    fn encode_factory_mint_for_asset_call_data_encodes_selector_and_words() {
        let data =
            encode_factory_mint_for_asset_call_data(&[0x33u8; 29], &[0x11u8; 20], &[0x22u8; 32])
                .expect("encode");
        assert_eq!(data.len(), 164);
        assert_ne!(&data[0..4], &[0u8; 4]);
        assert_eq!(&data[4..36], &u256_from_u64(96));
        assert_eq!(&data[36..48], &[0u8; 12]);
        assert_eq!(&data[48..68], &[0x11u8; 20]);
        assert_eq!(&data[68..100], &[0x22u8; 32]);
        assert_eq!(&data[100..132], &u256_from_u64(29));
        assert_eq!(&data[132..161], &[0x33u8; 29]);
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
            mint_failed_recoverable: false,
        };
        let bytes = encode_one(&value).expect("encode");
        let decoded: WrapRequestResult = decode_one(&bytes).expect("decode");
        assert!(decoded.withdrawn);
        assert_eq!(decoded.withdraw_ledger_tx_id, Some(vec![2u8; 4]));
        assert_eq!(
            decoded.withdraw_error_code.as_deref(),
            Some("withdraw.call_failed:oops")
        );
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
            evm_nonce: 1,
            gas_limit: 300_000,
            result: WrapRequestResult {
                status: RequestStatus::Failed,
                pull_ledger_tx_id: Some(vec![1u8; 4]),
                mint_tx_id: None,
                error_code: Some("evm_gateway.submit_failed:rejected:nonce".to_string()),
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                mint_failed_recoverable: true,
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
            evm_nonce: 1,
            gas_limit: 300_000,
            result: WrapRequestResult {
                status: RequestStatus::Failed,
                pull_ledger_tx_id: Some(vec![1u8; 4]),
                mint_tx_id: None,
                error_code: Some("mint_failed".to_string()),
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                mint_failed_recoverable: true,
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
    }

    #[test]
    fn is_withdrawable_matches_expected_shape() {
        let owner = Principal::self_authenticating(b"wrap-owner");
        let req = WrapStoredRequest {
            caller: owner.as_slice().to_vec(),
            asset_id: vec![7u8; 29],
            amount: vec![0u8; 32],
            evm_recipient: vec![9u8; 20],
            evm_nonce: 1,
            gas_limit: 300_000,
            result: WrapRequestResult {
                status: RequestStatus::Failed,
                pull_ledger_tx_id: Some(vec![1u8; 4]),
                mint_tx_id: None,
                error_code: Some("mint_failed".to_string()),
                withdrawn: false,
                withdraw_ledger_tx_id: None,
                withdraw_error_code: None,
                mint_failed_recoverable: true,
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
}
