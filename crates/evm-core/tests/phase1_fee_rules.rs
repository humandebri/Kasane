//! どこで: Phase1.3テスト / 何を: fee境界とbase_fee再評価 / なぜ: 有効手数料と順序の決定性を保証するため

use evm_core::chain::{self, ChainError};
use evm_db::chain_data::constants::DROP_CODE_INVALID_FEE;
use evm_db::chain_data::TxLocKind;
use evm_db::stable_state::{init_stable_state, with_state_mut};

#[test]
fn min_priority_fee_rejects_low_tip() {
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 2_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx_bytes = build_ic_tx_bytes(3_000_000_000, 1_000_000_000, 0);
    let err = chain::submit_ic_tx([0x11u8; 20], vec![0x11], vec![0x01], tx_bytes)
        .expect_err("submit should fail");
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

    let tx_bytes = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 0);
    let tx_id = chain::submit_ic_tx([0x22u8; 20], vec![0x22], vec![0x02], tx_bytes)
        .expect("submit");

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 3_000_000_000;
        state.chain_state.set(chain_state);
    });

    let err = chain::produce_block(1).expect_err("produce should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_INVALID_FEE);
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

    let tx_a = build_ic_tx_bytes(6_000_000_000, 3_000_000_000, 0);
    let tx_b = build_ic_tx_bytes(10_000_000_000, 2_000_000_000, 0);

    let a_id = chain::submit_ic_tx([0x33u8; 20], vec![0x33], vec![0x03], tx_a).expect("submit a");
    let b_id = chain::submit_ic_tx([0x44u8; 20], vec![0x44], vec![0x04], tx_b).expect("submit b");

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 5_000_000_000;
        state.chain_state.set(chain_state);
    });

    let block = chain::produce_block(2).expect("produce");
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

    let tx_a = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 0);
    let tx_b = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 0);

    let a_id = chain::submit_ic_tx([0x55u8; 20], vec![0x55], vec![0x05], tx_a).expect("submit a");
    let b_id = chain::submit_ic_tx([0x66u8; 20], vec![0x66], vec![0x06], tx_b).expect("submit b");

    let block = chain::produce_block(2).expect("produce");
    assert_eq!(block.tx_ids.len(), 2);
    assert_eq!(block.tx_ids[0], a_id);
    assert_eq!(block.tx_ids[1], b_id);
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
