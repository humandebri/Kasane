//! どこで: Phase1テスト / 何を: TxIn入口の最小検証 / なぜ: submit経路の統一で退行を防ぐため

use evm_core::chain::{self, ChainError, TxIn};
use evm_core::tx_decode::IcSyntheticTxInput;
use evm_db::chain_data::{TxKind, TxLocKind};
use evm_db::stable_state::{init_stable_state, with_state_mut};

mod common;

fn relax_fee_floor_for_tests() {
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1;
        chain_state.min_gas_price = 1;
        chain_state.min_priority_fee = 1;
        state.chain_state.set(chain_state);
    });
}

#[test]
fn submit_tx_in_eth_keeps_existing_decode_rules() {
    init_stable_state();
    let err = chain::submit_tx_in(TxIn::EthSigned {
        tx_bytes: vec![0x02, 0x01, 0x02],
        caller_principal: vec![0x01],
    })
    .unwrap_err();
    assert_eq!(err, ChainError::DecodeFailed);
}

#[test]
fn submit_tx_in_ic_synthetic_enqueues_tx() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let caller_principal = vec![0x42];
    let canister_id = vec![0x99];
    let tx_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: caller_principal.clone(),
        canister_id: canister_id.clone(),
        tx: common::build_ic_tx_input([0x11u8; 20], 0, 2_000_000_000, 1_000_000_000),
    })
    .expect("submit ic tx");

    let envelope = chain::get_tx_envelope(&tx_id).expect("stored tx");
    assert_eq!(envelope.kind, TxKind::IcSynthetic);
    assert_eq!(envelope.caller_principal, caller_principal);
    assert_eq!(envelope.canister_id, canister_id);
    let loc = chain::get_tx_loc(&tx_id).expect("tx location");
    assert_eq!(loc.kind, TxLocKind::Queued);
}

#[test]
fn submit_ic_tx_duplicate_returns_tx_already_seen() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let caller_principal = vec![0x42];
    let canister_id = vec![0x99];
    let tx = common::build_ic_tx_input([0x11u8; 20], 0, 2_000_000_000, 1_000_000_000);

    let _ = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: caller_principal.clone(),
        canister_id: canister_id.clone(),
        tx: tx.clone(),
    })
    .expect("first submit");
    let err = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal,
        canister_id,
        tx,
    })
    .expect_err("duplicate");
    assert_eq!(err, ChainError::TxAlreadySeen);
}

#[test]
fn submit_tx_in_ic_synthetic_rejects_oversized_payload() {
    init_stable_state();
    relax_fee_floor_for_tests();
    let err = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x51],
        canister_id: vec![0x71],
        tx: IcSyntheticTxInput {
            to: Some([0x11u8; 20]),
            value: [0u8; 32],
            gas_limit: 21_000,
            nonce: 0,
            max_fee_per_gas: 2_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            data: vec![0u8; evm_db::chain_data::constants::MAX_TX_SIZE.saturating_add(1)],
        },
    })
    .expect_err("oversized payload must fail");
    assert_eq!(err, ChainError::TxTooLarge);
}
