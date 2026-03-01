//! どこで: Phase1.3テスト / 何を: 手数料順の優先選択 / なぜ: FEE_SORTEDの決定性を保証するため

use evm_core::chain::{self, TxIn};
use evm_core::hash;
use evm_db::stable_state::{init_stable_state, with_state_mut};

mod common;

#[test]
fn fee_sorted_prefers_higher_effective_fee() {
    init_stable_state();

    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        chain_state.min_gas_price = 1;
        chain_state.min_priority_fee = 1;
        state.chain_state.set(chain_state);
    });

    common::fund_account(
        hash::derive_evm_address_from_principal(&[0x11]).expect("must derive"),
        1_000_000_000_000_000_000,
    );
    common::fund_account(
        hash::derive_evm_address_from_principal(&[0x22]).expect("must derive"),
        1_000_000_000_000_000_000,
    );

    let high_fee_tx = common::build_zero_to_ic_tx_input(0, 2_000_000_000, 1_000_000_000);
    let low_fee_tx = common::build_zero_to_ic_tx_input(0, 1_500_000_000, 1_000_000_000);

    let high_tx_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x11],
        canister_id: vec![0x01],
        tx: high_fee_tx,
    })
    .expect("submit high");
    let low_tx_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x22],
        canister_id: vec![0x02],
        tx: low_fee_tx,
    })
    .expect("submit low");

    let outcome = chain::produce_block(2).expect("produce");
    let block = outcome.block;
    assert_eq!(block.tx_ids.len(), 2);
    assert_eq!(block.tx_ids[0], high_tx_id);
    assert_eq!(block.tx_ids[1], low_tx_id);
}
