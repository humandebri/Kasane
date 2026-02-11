//! どこで: Phase1挙動テスト / 何を: metricsと選抜順序の回帰検知 / なぜ: chain.rs内テストを外出しして保守性を上げるため

use evm_core::chain;
use evm_core::hash;
use evm_db::stable_state::{init_stable_state, with_state};

mod common;

#[test]
fn execute_ic_tx_invalid_opcode_does_not_increment_unknown_halt_metrics() {
    init_stable_state();
    let caller_principal = vec![0x42];
    let caller = hash::caller_evm_from_principal(&caller_principal);
    common::fund_account(caller, 1_000_000_000_000_000_000);

    let halt_target = [0x12u8; 20];
    common::install_contract(halt_target, &[0xfe]); // INVALID

    let (_, first) = common::execute_ic_tx_via_produce(
        caller_principal.clone(),
        vec![0xaa],
        common::build_ic_tx_bytes(halt_target, 0, 2_000_000_000, 1_000_000_000),
    );
    assert_eq!(first.status, 0);

    let first_metrics = with_state(|state| *state.ops_metrics.get());
    assert_eq!(first_metrics.exec_halt_unknown_count, 0);
    assert_eq!(first_metrics.last_exec_halt_unknown_warn_ts, 0);

    let (_, second) = common::execute_ic_tx_via_produce(
        caller_principal,
        vec![0xbb],
        common::build_ic_tx_bytes(halt_target, 1, 2_000_000_000, 1_000_000_000),
    );
    assert_eq!(second.status, 0);

    let second_metrics = with_state(|state| *state.ops_metrics.get());
    assert_eq!(second_metrics.exec_halt_unknown_count, 0);
    assert_eq!(second_metrics.last_exec_halt_unknown_warn_ts, 0);
}

#[test]
fn produce_block_selects_top_k_by_fee_then_submission_order() {
    init_stable_state();

    let mut submitted: Vec<([u8; 32], u128, usize)> = Vec::new();
    let fees = [
        2_000_000_000u128,
        4_000_000_000,
        3_000_000_000,
        4_000_000_000,
        5_000_000_000,
        2_000_000_000,
        4_000_000_000,
        2_500_000_000,
    ];

    for (idx, fee) in fees.iter().copied().enumerate() {
        let idx_u8 = u8::try_from(idx).unwrap_or(0);
        let tx_id = chain::submit_ic_tx(
            vec![0x10 + idx_u8],
            vec![0x80 + idx_u8],
            common::build_ic_tx_bytes([0x20 + idx_u8; 20], 0, fee, fee),
        )
        .expect("submit");
        submitted.push((tx_id.0, fee, idx));
    }

    let produced = chain::produce_block(5).expect("produce");
    let selected = produced.block.tx_ids;
    assert_eq!(selected.len(), 5);

    let mut expected = submitted;
    expected.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.0.cmp(&right.0))
    });
    let expected_ids: Vec<[u8; 32]> = expected.into_iter().take(5).map(|value| value.0).collect();
    let selected_ids: Vec<[u8; 32]> = selected.into_iter().map(|value| value.0).collect();
    assert_eq!(selected_ids, expected_ids);
}
