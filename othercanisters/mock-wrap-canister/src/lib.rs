//! どこで: unwrap duplicate 復旧テスト用 mock wrap canister
//! 何を: submit_unwrap_request と get_request_status の最小実装
//! なぜ: gateway upgrade 中の in-flight dispatch を安定再現するため

use candid::{CandidType, Deserialize, Nat, Principal};
use ic_cdk::api::canister_self;
use ic_evm_rpc_types::{ApiError, ApiErrorDetail};
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct DispatchUnwrapRequestArgs {
    request_id: Vec<u8>,
    asset_id: Principal,
    amount_e8s: Nat,
    recipient: Principal,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct DispatchUnwrapRequestOk {
    request_id: Vec<u8>,
}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RequestStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct StoredRequest {
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    recipient: Vec<u8>,
    status: RequestStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InsertRequestOutcome {
    Inserted,
    AlreadyExists,
}

thread_local! {
    static REQUESTS: RefCell<BTreeMap<Vec<u8>, StoredRequest>> = const { RefCell::new(BTreeMap::new()) };
}

#[ic_cdk::init]
fn init() {}

#[ic_cdk::update]
async fn dispatch_unwrap_request(
    args: DispatchUnwrapRequestArgs,
) -> Result<DispatchUnwrapRequestOk, ApiError> {
    let request_id = args.request_id.clone();
    let outcome = REQUESTS.with(|requests| {
        let mut requests = requests.borrow_mut();
        if let Some(existing) = requests.get(&request_id) {
            let same_payload = existing.asset_id == args.asset_id.as_slice()
                && existing.amount == nat_to_32_bytes(&args.amount_e8s)
                && existing.recipient == args.recipient.as_slice();
            return if same_payload {
                Ok(InsertRequestOutcome::AlreadyExists)
            } else {
                Err("request.idempotency_mismatch".to_string())
            };
        }
        requests.insert(
            request_id.clone(),
            StoredRequest {
                asset_id: args.asset_id.as_slice().to_vec(),
                amount: nat_to_32_bytes(&args.amount_e8s),
                recipient: args.recipient.as_slice().to_vec(),
                status: RequestStatus::Queued,
            },
        );
        Ok(InsertRequestOutcome::Inserted)
    });
    let outcome = outcome.map_err(|code| ApiError::InvalidArgument(ApiErrorDetail {
        code: code.clone(),
        message: code,
    }))?;

    if outcome == InsertRequestOutcome::Inserted {
        // 1 round 遅延させて、gateway が Dispatching のまま upgrade される窓を作る。
        let _: () = ic_cdk::call::Call::unbounded_wait(canister_self(), "ping")
            .await
            .map_err(|err| api_internal(&err.to_string()))?
            .candid()
            .map_err(|err| api_internal(&err.to_string()))?;
    }

    Ok(DispatchUnwrapRequestOk { request_id })
}

#[ic_cdk::update]
fn ping() {}

#[derive(Clone, Copy, Debug, CandidType, Deserialize, Eq, PartialEq)]
enum RequestKind {
    Unwrap,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct RequestOverview {
    kind: RequestKind,
    request_id: Vec<u8>,
    status: RequestStatus,
}

#[ic_cdk::query]
fn get_request(request_id: Vec<u8>) -> Option<RequestOverview> {
    REQUESTS.with(|requests| {
        requests
            .borrow()
            .get(&request_id)
            .map(|req| RequestOverview {
                kind: RequestKind::Unwrap,
                request_id,
                status: req.status,
            })
    })
}

fn nat_to_32_bytes(value: &Nat) -> Vec<u8> {
    let mut out = [0u8; 32];
    let bytes = value.0.to_bytes_be();
    let start = 32usize.saturating_sub(bytes.len());
    out[start..start + bytes.len()].copy_from_slice(bytes.as_slice());
    out.to_vec()
}

fn api_internal(message: &str) -> ApiError {
    ApiError::Internal(ApiErrorDetail {
        code: message.to_string(),
        message: message.to_string(),
    })
}
