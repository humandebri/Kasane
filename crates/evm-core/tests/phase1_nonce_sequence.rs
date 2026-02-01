//! どこで: Phase1.3テスト / 何を: 同一senderのnonce順序 / なぜ: nonceゲートの基本動作を保証するため

use evm_core::chain::{self, ChainError};
use evm_db::chain_data::constants::DROP_CODE_REPLACED;
use evm_db::chain_data::TxLocKind;
use evm_db::stable_state::{init_stable_state, with_state_mut};

#[test]
fn sequential_nonces_are_included_across_blocks() {
    init_stable_state();

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 1_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx0 = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 0);
    let tx1 = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 1);

    let id0 = chain::submit_ic_tx(vec![0x77], vec![0x07], tx0).expect("submit 0");
    let block1 = chain::produce_block(1).expect("produce block1");
    assert_eq!(block1.tx_ids.len(), 1);
    assert_eq!(block1.tx_ids[0], id0);

    let id1 = chain::submit_ic_tx(vec![0x77], vec![0x07], tx1).expect("submit 1");
    let block2 = chain::produce_block(1).expect("produce block2");
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

    let tx1 = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 1);
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

    let tx0 = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 0);
    chain::submit_ic_tx(vec![0x99], vec![0x09], tx0).expect("submit 0");
    let _ = chain::produce_block(1).expect("produce block");

    let tx0_again = build_ic_tx_bytes(3_000_000_000, 1_000_000_000, 0);
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

    let low_fee = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 0);
    let high_fee = build_ic_tx_bytes(3_000_000_000, 2_000_000_000, 0);

    let low_id = chain::submit_ic_tx(vec![0xaa], vec![0x0a], low_fee).expect("submit low");
    let high_id = chain::submit_ic_tx(vec![0xaa], vec![0x0a], high_fee).expect("submit high");

    let low_loc = chain::get_tx_loc(&low_id).expect("low tx loc");
    assert_eq!(low_loc.kind, TxLocKind::Dropped);
    assert_eq!(low_loc.drop_code, DROP_CODE_REPLACED);

    let block = chain::produce_block(1).expect("produce");
    assert_eq!(block.tx_ids.len(), 1);
    assert_eq!(block.tx_ids[0], high_id);
}

fn build_ic_tx_bytes(max_fee: u128, max_priority: u128, nonce: u64) -> Vec<u8> {
    let to = [0u8; 20];
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce = nonce.to_be_bytes();
    let max_fee = max_fee.to_be_bytes();
    let max_priority = max_priority.to_be_bytes();
    let data: Vec<u8> = Vec::new();
    let data_len = u32::try_from(data.len()).unwrap_or(0).to_be_bytes();
    let mut out = Vec::new();
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&value);
    out.extend_from_slice(&gas_limit);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&max_fee);
    out.extend_from_slice(&max_priority);
    out.extend_from_slice(&data_len);
    out.extend_from_slice(&data);
    out
}
