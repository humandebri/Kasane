//! どこで: unwrap duplicate 復旧テスト用 mock wrap canister
//! 何を: submit_unwrap_request と get_request_status の最小実装
//! なぜ: gateway upgrade 中の in-flight dispatch を安定再現するため

use candid::{CandidType, Deserialize};
use ic_cdk::api::canister_self;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitUnwrapRequestArgs {
    request_id: Vec<u8>,
    asset_id: Vec<u8>,
    amount: Vec<u8>,
    recipient: Vec<u8>,
}

#[derive(Clone, Debug, CandidType, Deserialize, Eq, PartialEq)]
struct SubmitUnwrapRequestOk {
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
async fn submit_unwrap_request(
    args: SubmitUnwrapRequestArgs,
) -> Result<SubmitUnwrapRequestOk, String> {
    let request_id = args.request_id.clone();
    let outcome = REQUESTS.with(|requests| {
        let mut requests = requests.borrow_mut();
        if let Some(existing) = requests.get(&request_id) {
            let same_payload = existing.asset_id == args.asset_id
                && existing.amount == args.amount
                && existing.recipient == args.recipient;
            return if same_payload {
                Ok(InsertRequestOutcome::AlreadyExists)
            } else {
                Err("request.idempotency_mismatch".to_string())
            };
        }
        requests.insert(
            request_id.clone(),
            StoredRequest {
                asset_id: args.asset_id,
                amount: args.amount,
                recipient: args.recipient,
                status: RequestStatus::Queued,
            },
        );
        Ok(InsertRequestOutcome::Inserted)
    });
    let outcome = outcome?;

    if outcome == InsertRequestOutcome::Inserted {
        // 1 round 遅延させて、gateway が Dispatching のまま upgrade される窓を作る。
        let _: () = ic_cdk::call::Call::unbounded_wait(canister_self(), "ping")
            .await
            .map_err(|err| err.to_string())?
            .candid()
            .map_err(|err| err.to_string())?;
    }

    Ok(SubmitUnwrapRequestOk { request_id })
}

#[ic_cdk::update]
fn ping() {}

#[ic_cdk::query]
fn get_request_status(request_id: Vec<u8>) -> Option<RequestStatus> {
    REQUESTS.with(|requests| requests.borrow().get(&request_id).map(|req| req.status))
}
