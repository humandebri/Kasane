//! どこで: Phase1.3テスト / 何を: 同一senderのnonce順序 / なぜ: nonceゲートの基本動作を保証するため

use evm_core::chain::{self, ChainError};
use evm_core::hash;
use evm_db::chain_data::constants::DROP_CODE_REPLACED;
use evm_db::chain_data::TxLocKind;
use evm_db::stable_state::{init_stable_state, with_state_mut};

mod common;

fn fund_principal(principal: &[u8]) {
    common::fund_account(
        hash::derive_evm_address_from_principal(principal).expect("must derive"),
        1_000_000_000_000_000_000,
    );
}

#[test]
fn sequential_nonces_are_included_across_blocks() {
    init_stable_state();

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx0 = common::build_zero_to_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    let tx1 = common::build_zero_to_ic_tx_bytes(1, 2_000_000_000, 1_000_000_000);

    fund_principal(&[0x77]);
    let id0 = chain::submit_ic_tx(vec![0x77], vec![0x07], tx0).expect("submit 0");
    let outcome1 = chain::produce_block(1).expect("produce block1");
    let block1 = outcome1.block;
    assert_eq!(block1.tx_ids.len(), 1);
    assert_eq!(block1.tx_ids[0], id0);

    let id1 = chain::submit_ic_tx(vec![0x77], vec![0x07], tx1).expect("submit 1");
    let outcome2 = chain::produce_block(1).expect("produce block2");
    let block2 = outcome2.block;
    assert_eq!(block2.tx_ids.len(), 1);
    assert_eq!(block2.tx_ids[0], id1);
}

#[test]
fn nonce_gap_is_rejected() {
    init_stable_state();

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx1 = common::build_zero_to_ic_tx_bytes(1, 2_000_000_000, 1_000_000_000);
    let err = chain::submit_ic_tx(vec![0x88], vec![0x08], tx1).expect_err("nonce gap");
    assert_eq!(err, ChainError::NonceGap);
}

#[test]
fn nonce_too_low_is_rejected() {
    init_stable_state();

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx0 = common::build_zero_to_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    fund_principal(&[0x99]);
    chain::submit_ic_tx(vec![0x99], vec![0x09], tx0).expect("submit 0");
    let _ = chain::produce_block(1).expect("produce block");

    let tx0_again = common::build_zero_to_ic_tx_bytes(0, 3_000_000_000, 1_000_000_000);
    let err = chain::submit_ic_tx(vec![0x99], vec![0x09], tx0_again).expect_err("nonce too low");
    assert_eq!(err, ChainError::NonceTooLow);
}

#[test]
fn replacement_requires_higher_effective_fee() {
    init_stable_state();

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        state.chain_state.set(chain_state);
    });

    let low_fee = common::build_zero_to_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    let high_fee = common::build_zero_to_ic_tx_bytes(0, 3_000_000_000, 2_000_000_000);

    fund_principal(&[0xaa]);
    let low_id = chain::submit_ic_tx(vec![0xaa], vec![0x0a], low_fee).expect("submit low");
    let high_id = chain::submit_ic_tx(vec![0xaa], vec![0x0a], high_fee).expect("submit high");

    let low_loc = chain::get_tx_loc(&low_id).expect("low tx loc");
    assert_eq!(low_loc.kind, TxLocKind::Dropped);
    assert_eq!(low_loc.drop_code, DROP_CODE_REPLACED);

    let outcome = chain::produce_block(1).expect("produce");
    let block = outcome.block;
    assert_eq!(block.tx_ids.len(), 1);
    assert_eq!(block.tx_ids[0], high_id);
}
