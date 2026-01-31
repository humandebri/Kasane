//! どこで: Phase1.3テスト / 何を: 同一senderのnonce順序 / なぜ: nonceゲートの基本動作を保証するため

use evm_core::chain;
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

    let caller = [0x77u8; 20];
    let tx0 = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 0);
    let tx1 = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 1);

    let id0 = chain::submit_ic_tx(caller, vec![0x77], vec![0x07], tx0).expect("submit 0");
    let id1 = chain::submit_ic_tx(caller, vec![0x77], vec![0x07], tx1).expect("submit 1");

    let block1 = chain::produce_block(1).expect("produce block1");
    assert_eq!(block1.tx_ids.len(), 1);
    assert_eq!(block1.tx_ids[0], id0);

    let block2 = chain::produce_block(1).expect("produce block2");
    assert_eq!(block2.tx_ids.len(), 1);
    assert_eq!(block2.tx_ids[0], id1);
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
