//! どこで: Phase1永続化境界テスト / 何を: storage write失敗時のtrapロールバック検証 / なぜ: 部分コミットを防ぐため

use evm_core::chain;
use evm_core::hash;
use evm_db::stable_state::{init_stable_state, with_state};
use std::panic::{self, AssertUnwindSafe};

mod common;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Snapshot {
    head_number: u64,
    blocks_len: u64,
    receipts_len: u64,
    tx_index_len: u64,
    tx_locs_len: u64,
}

struct FailpointGuard;

impl Drop for FailpointGuard {
    fn drop(&mut self) {
        chain::configure_store_failpoint_for_test(None);
    }
}

#[test]
fn produce_block_traps_and_rolls_back_when_receipt_store_fails_after_tx_index() {
    init_stable_state();
    let tx_id = chain::submit_ic_tx(vec![0x11], vec![0x21], common::build_default_ic_tx_bytes(0))
        .expect("submit");
    let before = snapshot();
    assert!(with_state(|state| state.tx_locs.get(&tx_id).is_some()));

    let _guard = FailpointGuard;
    chain::configure_store_failpoint_for_test(Some(2));
    assert_trap_happened(|| {
        let _ = chain::produce_block(1);
    });
    assert_eq!(snapshot(), before);
}

#[test]
fn produce_block_traps_and_rolls_back_when_block_store_fails_after_receipt() {
    init_stable_state();
    let tx_id = chain::submit_ic_tx(vec![0x12], vec![0x22], common::build_default_ic_tx_bytes(0))
        .expect("submit");
    let before = snapshot();
    assert!(with_state(|state| state.tx_locs.get(&tx_id).is_some()));

    let _guard = FailpointGuard;
    chain::configure_store_failpoint_for_test(Some(3));
    assert_trap_happened(|| {
        let _ = chain::produce_block(1);
    });
    assert_eq!(snapshot(), before);
}

#[test]
fn execute_and_seal_traps_and_rolls_back_when_tx_index_store_fails_after_block() {
    init_stable_state();
    let caller_principal = vec![0x33];
    let canister_id = vec![0x44];
    let caller_evm = hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller_evm, 1_000_000_000_000_000_000).expect("mint");
    let tx_id = chain::submit_ic_tx(
        caller_principal.clone(),
        canister_id.clone(),
        common::build_default_ic_tx_bytes(0),
    )
    .expect("submit");
    let before = snapshot();
    assert!(with_state(|state| state.tx_locs.get(&tx_id).is_some()));

    let _guard = FailpointGuard;
    chain::configure_store_failpoint_for_test(Some(2));
    assert_trap_happened(|| {
        let _ = chain::execute_submitted_ic_tx_for_test(tx_id, caller_evm);
    });
    assert_eq!(snapshot(), before);
}

#[test]
fn execute_and_seal_traps_and_rolls_back_when_receipt_store_fails_after_tx_index() {
    init_stable_state();
    let caller_principal = vec![0x35];
    let canister_id = vec![0x45];
    let caller_evm = hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller_evm, 1_000_000_000_000_000_000).expect("mint");
    let tx_id = chain::submit_ic_tx(
        caller_principal.clone(),
        canister_id.clone(),
        common::build_default_ic_tx_bytes(0),
    )
    .expect("submit");
    let before = snapshot();
    assert!(with_state(|state| state.tx_locs.get(&tx_id).is_some()));

    let _guard = FailpointGuard;
    chain::configure_store_failpoint_for_test(Some(3));
    assert_trap_happened(|| {
        let _ = chain::execute_submitted_ic_tx_for_test(tx_id, caller_evm);
    });
    assert_eq!(snapshot(), before);
}

fn snapshot() -> Snapshot {
    with_state(|state| Snapshot {
        head_number: state.head.get().number,
        blocks_len: state.blocks.len(),
        receipts_len: state.receipts.len(),
        tx_index_len: state.tx_index.len(),
        tx_locs_len: state.tx_locs.len(),
    })
}

fn assert_trap_happened<F>(f: F)
where
    F: FnOnce(),
{
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));
    let out = panic::catch_unwind(AssertUnwindSafe(f));
    panic::set_hook(previous_hook);
    assert!(out.is_err(), "must panic from trap");
}
