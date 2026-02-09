//! どこで: Phase1.3テスト / 何を: fee境界とbase_fee再評価 / なぜ: 有効手数料と順序の決定性を保証するため

use alloy_eips::eip1559::{calc_next_block_base_fee, BaseFeeParams};
use evm_core::base_fee::compute_next_base_fee;
use evm_core::chain::{self, ChainError};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};

mod common;

#[test]
fn min_priority_fee_rejects_low_tip() {
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 2_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx_bytes = common::build_zero_to_ic_tx_bytes(0, 3_000_000_000, 1_000_000_000);
    let err =
        chain::submit_ic_tx(vec![0x11], vec![0x01], tx_bytes).expect_err("submit should fail");
    assert_eq!(err, ChainError::InvalidFee);
}

#[test]
fn base_fee_rekey_drops_unaffordable_tx() {
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx_bytes = common::build_zero_to_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    let tx_id = chain::submit_ic_tx(vec![0x22], vec![0x02], tx_bytes).expect("submit");

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 3_000_000_000;
        state.chain_state.set(chain_state);
    });

    let err = chain::produce_block(1).expect_err("produce should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, evm_db::chain_data::TxLocKind::Dropped);
    assert_eq!(
        loc.drop_code,
        evm_db::chain_data::constants::DROP_CODE_INVALID_FEE
    );
}

#[test]
fn base_fee_rekey_reorders_by_effective_fee() {
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx_a = common::build_zero_to_ic_tx_bytes(0, 6_000_000_000, 3_000_000_000);
    let tx_b = common::build_zero_to_ic_tx_bytes(0, 10_000_000_000, 2_000_000_000);

    let a_id = chain::submit_ic_tx(vec![0x33], vec![0x03], tx_a).expect("submit a");
    let b_id = chain::submit_ic_tx(vec![0x44], vec![0x04], tx_b).expect("submit b");

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 5_000_000_000;
        state.chain_state.set(chain_state);
    });

    let outcome = chain::produce_block(2).expect("produce");
    let block = outcome.block;
    assert_eq!(block.tx_ids.len(), 2);
    assert_eq!(block.tx_ids[0], b_id);
    assert_eq!(block.tx_ids[1], a_id);
}

#[test]
fn equal_fee_uses_seq_order() {
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx_a = common::build_zero_to_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    let tx_b = common::build_zero_to_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);

    let a_id = chain::submit_ic_tx(vec![0x55], vec![0x05], tx_a).expect("submit a");
    let b_id = chain::submit_ic_tx(vec![0x66], vec![0x06], tx_b).expect("submit b");

    let outcome = chain::produce_block(2).expect("produce");
    let block = outcome.block;
    assert_eq!(block.tx_ids.len(), 2);
    assert_eq!(block.tx_ids[0], a_id);
    assert_eq!(block.tx_ids[1], b_id);
}

#[test]
fn base_fee_matches_alloy_reference_vectors() {
    let base_fee = [
        1_000_000_000u64,
        1_000_000_000,
        1_072_671_875,
        1_049_238_967,
        0,
        1,
    ];
    let gas_used = [
        10_000_000u64,
        9_000_000,
        9_000_000,
        0,
        10_000_000,
        10_000_000,
    ];
    let gas_limit = [
        10_000_000u64,
        10_000_000,
        10_000_000,
        2_000_000,
        18_000_000,
        18_000_000,
    ];
    for idx in 0..base_fee.len() {
        let expected = calc_next_block_base_fee(
            gas_used[idx],
            gas_limit[idx],
            base_fee[idx],
            BaseFeeParams::ethereum(),
        );
        let actual = compute_next_base_fee(base_fee[idx], gas_used[idx], gas_limit[idx]);
        assert_eq!(actual, expected, "vector idx={idx}");
    }
}

#[test]
fn base_fee_keeps_value_when_gas_target_is_zero() {
    let current = 1_000_000_000u64;
    let next = compute_next_base_fee(current, 1, 1);
    assert_eq!(next, current);
}

#[test]
fn produce_block_base_fee_uses_configured_block_gas_limit() {
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        chain_state.block_gas_limit = 8_000_000;
        state.chain_state.set(chain_state);
    });

    let tx = common::build_zero_to_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    let _ = chain::submit_ic_tx(vec![0x77], vec![0x07], tx).expect("submit");
    let outcome = chain::produce_block(1).expect("produce");
    let next_base_fee = with_state(|state| state.chain_state.get().base_fee);
    let expected = compute_next_base_fee(1_000_000_000, outcome.gas_used, 8_000_000);
    assert_eq!(next_base_fee, expected);
}
