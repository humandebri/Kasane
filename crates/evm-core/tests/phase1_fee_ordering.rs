//! どこで: Phase1.3テスト / 何を: 手数料順の優先選択 / なぜ: FEE_SORTEDの決定性を保証するため

use evm_core::chain;
use evm_db::stable_state::{init_stable_state, with_state_mut};

mod common;

#[test]
fn fee_sorted_prefers_higher_effective_fee() {
    init_stable_state();

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        state.chain_state.set(chain_state);
    });

    let high_fee_tx = common::build_zero_to_ic_tx_bytes(0, 2_000_000_000, 1_000_000_000);
    let low_fee_tx = common::build_zero_to_ic_tx_bytes(0, 1_500_000_000, 1_000_000_000);

    let high_tx_id = chain::submit_ic_tx(vec![0x11], vec![0x01], high_fee_tx).expect("submit high");
    let low_tx_id = chain::submit_ic_tx(vec![0x22], vec![0x02], low_fee_tx).expect("submit low");

    let outcome = chain::produce_block(2).expect("produce");
    let block = outcome.block;
    assert_eq!(block.tx_ids.len(), 2);
    assert_eq!(block.tx_ids[0], high_tx_id);
    assert_eq!(block.tx_ids[1], low_tx_id);
}
