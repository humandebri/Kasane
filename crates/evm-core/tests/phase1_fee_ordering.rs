//! どこで: Phase1.3テスト / 何を: 手数料順の優先選択 / なぜ: FEE_SORTEDの決定性を保証するため

use evm_core::chain;
use evm_db::stable_state::{init_stable_state, with_state_mut};

#[test]
fn fee_sorted_prefers_higher_effective_fee() {
    init_stable_state();

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        state.chain_state.set(chain_state);
    });

    let high_fee_tx = build_ic_tx_bytes(2_000_000_000, 1_000_000_000, 0);
    let low_fee_tx = build_ic_tx_bytes(1_500_000_000, 1_000_000_000, 0);

    let high_tx_id = chain::submit_ic_tx(
        [0x11u8; 20],
        vec![0x11],
        vec![0x01],
        high_fee_tx,
    )
    .expect("submit high");
    let low_tx_id = chain::submit_ic_tx([0x22u8; 20], vec![0x22], vec![0x02], low_fee_tx)
        .expect("submit low");

    let block = chain::produce_block(2).expect("produce");
    assert_eq!(block.tx_ids.len(), 2);
    assert_eq!(block.tx_ids[0], high_tx_id);
    assert_eq!(block.tx_ids[1], low_tx_id);
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
