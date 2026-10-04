//! Test ledger: suspend an actual inter-canister reply while stale recovery runs.
use candid::{CandidType, Nat};
use serde::Deserialize;
use std::cell::RefCell;

#[derive(Clone, Copy, CandidType, Deserialize, PartialEq)]
enum DelayMode {
    Pull,
    Metadata,
    FailedMetadata,
}
#[derive(Default, Clone, CandidType, Deserialize)]
struct DelayState {
    pulls: u64,
    transfer_attempts: u64,
    metadata_calls: u64,
}
#[derive(Clone, PartialEq, CandidType, Deserialize)]
struct TransferArgs {
    amount: Nat,
    memo: Option<Vec<u8>>,
    created_at_time: Option<u64>,
}
#[derive(CandidType, Deserialize)]
enum TransferError {
    Duplicate { duplicate_of: Nat },
}
#[derive(CandidType, Deserialize)]
enum Metadata {
    Nat(Nat),
}
thread_local! {
    static STATE: RefCell<DelayState> = RefCell::new(DelayState::default());
    static MODE: RefCell<DelayMode> = const { RefCell::new(DelayMode::Metadata) };
    static TRANSFER: RefCell<Option<TransferArgs>> = const { RefCell::new(None) };
    static RELEASED: RefCell<bool> = const { RefCell::new(false) };
}
#[ic_cdk::update]
fn configure(mode: DelayMode) {
    MODE.with(|m| *m.borrow_mut() = mode);
}
#[ic_cdk::update]
fn release() {
    RELEASED.with(|r| *r.borrow_mut() = true);
}
#[ic_cdk::query]
fn delay_state() -> DelayState {
    STATE.with(|s| s.borrow().clone())
}
async fn wait_for_release() {
    while !RELEASED.with(|r| *r.borrow()) {
        // Use a real IC call boundary so the original reply context stays alive.
        ic_cdk::call::Call::unbounded_wait(candid::Principal::management_canister(), "raw_rand")
            .await
            .expect("raw_rand");
    }
}
#[ic_cdk::update]
async fn icrc2_transfer_from(args: TransferArgs) -> Result<Nat, TransferError> {
    let first = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.transfer_attempts += 1;
        if s.pulls == 0 {
            s.pulls = 1;
            true
        } else {
            false
        }
    });
    TRANSFER.with(|saved| {
        let mut saved = saved.borrow_mut();
        if let Some(previous) = saved.as_ref() {
            assert!(previous == &args, "retry transfer identity changed");
        } else {
            *saved = Some(args);
        }
    });
    if !first {
        return Err(TransferError::Duplicate {
            duplicate_of: Nat::from(0u8),
        });
    }
    if MODE.with(|m| *m.borrow() == DelayMode::Pull) {
        wait_for_release().await;
    }
    Ok(Nat::from(0u8))
}
#[ic_cdk::update]
async fn icrc1_metadata() -> Vec<(String, Metadata)> {
    let first = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.metadata_calls += 1;
        s.metadata_calls == 1
    });
    let mode = MODE.with(|m| *m.borrow());
    if first && mode != DelayMode::Pull {
        wait_for_release().await;
    }
    let decimals = if first && mode == DelayMode::FailedMetadata {
        300u64
    } else {
        8
    };
    vec![("icrc1:decimals".into(), Metadata::Nat(Nat::from(decimals)))]
}
