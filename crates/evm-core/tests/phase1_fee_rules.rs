//! どこで: Phase1.3テスト / 何を: fee境界とbase_fee再評価 / なぜ: 有効手数料と順序の決定性を保証するため

use alloy_consensus::{SignableTransaction, TxEip1559, TxEip2930, TxLegacy};
use alloy_eips::eip1559::{calc_next_block_base_fee, BaseFeeParams};
use alloy_eips::eip2718::Encodable2718;
use alloy_eips::eip2930::AccessList;
use alloy_primitives::{Address, Bytes, TxKind as EthTxKind, U256 as AlloyU256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use evm_core::base_fee::compute_next_base_fee;
use evm_core::chain::{self, ChainError, TxIn};
use evm_core::hash;
use evm_db::chain_data::constants::CHAIN_ID;
use evm_db::chain_data::TxKind;
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::make_account_key;
use revm::primitives::U256;

mod common;

fn fund_principal(principal: &[u8]) {
    common::fund_account(
        hash::derive_evm_address_from_principal(principal).expect("must derive"),
        1_000_000_000_000_000_000,
    );
}

#[test]
fn min_priority_fee_rejects_low_tip() {
    init_stable_state();
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = 1_000_000_000;
        chain_state.min_priority_fee = 2_000_000_000;
        state.chain_state.set(chain_state);
    });

    let tx = common::build_zero_to_ic_tx_input(0, 3_000_000_000, 1_000_000_000);
    let err = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x11],
        canister_id: vec![0x01],
        tx,
    })
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

    let tx = common::build_zero_to_ic_tx_input(0, 2_000_000_000, 1_000_000_000);
    let tx_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x22],
        canister_id: vec![0x02],
        tx,
    })
    .expect("submit");

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

    let tx_a = common::build_zero_to_ic_tx_input(0, 6_000_000_000, 3_000_000_000);
    let tx_b = common::build_zero_to_ic_tx_input(0, 10_000_000_000, 2_000_000_000);

    fund_principal(&[0x33]);
    fund_principal(&[0x44]);
    let a_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x33],
        canister_id: vec![0x03],
        tx: tx_a,
    })
    .expect("submit a");
    let b_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x44],
        canister_id: vec![0x04],
        tx: tx_b,
    })
    .expect("submit b");

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

    let tx_a = common::build_zero_to_ic_tx_input(0, 2_000_000_000, 1_000_000_000);
    let tx_b = common::build_zero_to_ic_tx_input(0, 2_000_000_000, 1_000_000_000);

    fund_principal(&[0x55]);
    fund_principal(&[0x66]);
    let a_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x55],
        canister_id: vec![0x05],
        tx: tx_a,
    })
    .expect("submit a");
    let b_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x66],
        canister_id: vec![0x06],
        tx: tx_b,
    })
    .expect("submit b");

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

    let tx = common::build_zero_to_ic_tx_input(0, 2_000_000_000, 1_000_000_000);
    fund_principal(&[0x77]);
    let _ = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x77],
        canister_id: vec![0x07],
        tx,
    })
    .expect("submit");
    let outcome = chain::produce_block(1).expect("produce");
    let next_base_fee = with_state(|state| state.chain_state.get().base_fee);
    let expected = compute_next_base_fee(1_000_000_000, outcome.gas_used, 8_000_000);
    assert_eq!(next_base_fee, expected);
}

#[test]
fn zero_tip_transaction_still_credits_base_fee_to_fee_recipient() {
    init_stable_state();
    let base_fee = 1_000_000_000u64;
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = base_fee;
        chain_state.min_priority_fee = 0;
        state.chain_state.set(chain_state);
    });

    let recipient = evm_core::fee_recipient();
    let before = with_state(|state| {
        state
            .accounts
            .get(&make_account_key(recipient))
            .map(|account| U256::from_be_bytes(account.balance()))
            .unwrap_or(U256::ZERO)
    });

    let tx = common::build_zero_to_ic_tx_input(0, u128::from(base_fee), 0);
    fund_principal(&[0x88]);
    let tx_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: vec![0x88],
        canister_id: vec![0x08],
        tx,
    })
    .expect("submit");
    let _ = chain::produce_block(1).expect("produce");
    let receipt = chain::get_receipt(&tx_id).expect("receipt");

    let after = with_state(|state| {
        state
            .accounts
            .get(&make_account_key(recipient))
            .map(|account| U256::from_be_bytes(account.balance()))
            .unwrap_or(U256::ZERO)
    });

    let expected_delta = u128::from(receipt.gas_used).saturating_mul(u128::from(base_fee));
    assert_eq!(after, before.saturating_add(U256::from(expected_delta)));
    assert_eq!(receipt.total_fee, expected_delta);
}

#[test]
fn eth_signed_zero_tip_still_credits_base_fee_to_fee_recipient() {
    init_stable_state();
    let base_fee = 1_000_000_000u64;
    with_state_mut(|state| {
        let mut chain_state = *state.chain_state.get();
        chain_state.base_fee = base_fee;
        chain_state.min_priority_fee = 0;
        chain_state.min_gas_price = 0;
        state.chain_state.set(chain_state);
    });

    let recipient = evm_core::fee_recipient();
    let before = with_state(|state| {
        state
            .accounts
            .get(&make_account_key(recipient))
            .map(|account| U256::from_be_bytes(account.balance()))
            .unwrap_or(U256::ZERO)
    });

    let signer = test_signer();
    common::fund_account(signer.address().into_array(), 1_000_000_000_000_000_000);
    let raw = build_eth_signed_1559(0, u128::from(base_fee), 0);
    let tx_id = chain::submit_tx(TxKind::EthSigned, raw, vec![0x89]).expect("submit eth");
    let _ = chain::produce_block(1).expect("produce");
    let receipt = chain::get_receipt(&tx_id).expect("receipt");

    let after = with_state(|state| {
        state
            .accounts
            .get(&make_account_key(recipient))
            .map(|account| U256::from_be_bytes(account.balance()))
            .unwrap_or(U256::ZERO)
    });

    let expected_delta = u128::from(receipt.gas_used).saturating_mul(u128::from(base_fee));
    assert_eq!(after, before.saturating_add(U256::from(expected_delta)));
    assert_eq!(receipt.total_fee, expected_delta);
}

fn test_signer() -> PrivateKeySigner {
    "0x59c6995e998f97a5a0044966f094538e0d7f4f4e4d5d8dd6a8c4f9d5f8b1e8a1"
        .parse()
        .expect("signer")
}

fn build_eth_signed_1559(
    nonce: u64,
    max_fee_per_gas: u128,
    max_priority_fee_per_gas: u128,
) -> Vec<u8> {
    let signer = test_signer();
    let tx = TxEip1559 {
        chain_id: CHAIN_ID,
        nonce,
        gas_limit: 50_000,
        max_fee_per_gas,
        max_priority_fee_per_gas,
        to: EthTxKind::Call(Address::from([0x21u8; 20])),
        value: AlloyU256::ZERO,
        access_list: AccessList::default(),
        input: Bytes::new(),
    };
    let hash = tx.signature_hash();
    let signature = signer.sign_hash_sync(&hash).expect("sign");
    tx.into_signed(signature).encoded_2718()
}

fn build_eth_signed_fixed_price(nonce: u64, gas_price: u128, access_list: bool) -> Vec<u8> {
    let signer = test_signer();
    let legacy = TxLegacy {
        chain_id: Some(CHAIN_ID),
        nonce,
        gas_price,
        gas_limit: 21_000,
        to: EthTxKind::Call(Address::from([0x21u8; 20])),
        value: AlloyU256::ZERO,
        input: Bytes::new(),
    };
    if access_list {
        let tx = TxEip2930 {
            chain_id: CHAIN_ID,
            nonce,
            gas_price,
            gas_limit: legacy.gas_limit,
            to: legacy.to,
            value: legacy.value,
            access_list: AccessList::default(),
            input: legacy.input,
        };
        let signature = signer.sign_hash_sync(&tx.signature_hash()).expect("sign");
        tx.into_signed(signature).encoded_2718()
    } else {
        let signature = signer
            .sign_hash_sync(&legacy.signature_hash())
            .expect("sign");
        legacy.into_signed(signature).encoded_2718()
    }
}

#[test]
fn fixed_price_above_receipt_range_is_rejected_before_queueing() {
    for access_list in [false, true] {
        std::thread::spawn(move || {
            init_stable_state();
            for gas_price in [u128::from(u64::MAX) + 1, u128::MAX] {
                let raw = build_eth_signed_fixed_price(0, gas_price, access_list);
                assert_eq!(
                    chain::submit_tx(TxKind::EthSigned, raw, vec![0x90]),
                    Err(ChainError::InvalidFee)
                );
                with_state(|state| {
                    assert!(state.tx_store.is_empty());
                    assert!(state.seen_tx.is_empty());
                    assert!(state.pending_by_sender_nonce.is_empty());
                    assert!(state.sender_expected_nonce.is_empty());
                    assert!(state.pending_fee_index.is_empty());
                    assert!(state.ready_queue.is_empty());
                });
            }
            // Rejection must leave nonce 0 available for a valid submission.
            common::fund_account(test_signer().address().into_array(), u128::MAX);
            let raw = build_eth_signed_fixed_price(0, u128::from(u64::MAX), access_list);
            let id = chain::submit_tx(TxKind::EthSigned, raw, vec![0x90]).expect("submit boundary");
            chain::produce_block(1).expect("execute boundary");
            let receipt = chain::get_receipt(&id).expect("receipt");
            assert_eq!(receipt.status, 1);
            assert_eq!(receipt.effective_gas_price, u64::MAX);
            assert_eq!(
                receipt.total_fee,
                u128::from(receipt.gas_used) * u128::from(u64::MAX)
            );
        })
        .join()
        .unwrap();
    }
}

#[test]
fn fixed_price_replacement_uses_gas_price() {
    for access_list in [false, true] {
        std::thread::spawn(move || {
            init_stable_state();
            let base_fee = with_state(|state| state.chain_state.get().base_fee);
            let low_price = u128::from(base_fee) * 2;
            let high_price = low_price + u128::from(base_fee);
            common::fund_account(test_signer().address().into_array(), u128::MAX);
            let old = chain::submit_tx(
                TxKind::EthSigned,
                build_eth_signed_fixed_price(0, low_price, access_list),
                vec![0x91],
            )
            .expect("submit original");
            let replacement = chain::submit_tx(
                TxKind::EthSigned,
                build_eth_signed_fixed_price(0, high_price, access_list),
                vec![0x91],
            )
            .expect("replace with higher fixed price");
            assert_eq!(
                chain::submit_tx(
                    TxKind::EthSigned,
                    build_eth_signed_fixed_price(0, high_price - 1, access_list),
                    vec![0x91],
                ),
                Err(ChainError::NonceConflict)
            );
            let old_loc = chain::get_tx_loc(&old).expect("old location");
            assert_eq!(old_loc.kind, evm_db::chain_data::TxLocKind::Dropped);
            assert_eq!(
                old_loc.drop_code,
                evm_db::chain_data::constants::DROP_CODE_REPLACED
            );
            let outcome = chain::produce_block(1).expect("execute replacement");
            assert_eq!(outcome.block.tx_ids, vec![replacement]);
            assert_eq!(
                chain::get_receipt(&replacement)
                    .unwrap()
                    .effective_gas_price,
                u64::try_from(high_price).unwrap()
            );
        })
        .join()
        .unwrap();
    }
}

#[test]
fn fixed_price_keeps_priority_after_base_fee_change() {
    for access_list in [false, true] {
        std::thread::spawn(move || {
            init_stable_state();
            with_state_mut(|state| {
                let mut chain_state = *state.chain_state.get();
                chain_state.base_fee = 1;
                chain_state.min_gas_price = 1;
                chain_state.min_priority_fee = 1;
                state.chain_state.set(chain_state);
            });
            let caller = vec![0x92];
            fund_principal(&caller);
            common::fund_account(test_signer().address().into_array(), u128::MAX);
            let dynamic = chain::submit_tx_in(TxIn::IcSynthetic {
                caller_principal: caller,
                canister_id: vec![0x01],
                tx: common::build_zero_to_ic_tx_input(0, 10, 1),
            })
            .expect("submit dynamic");
            let fixed = chain::submit_tx(
                TxKind::EthSigned,
                build_eth_signed_fixed_price(0, 4, access_list),
                vec![0x93],
            )
            .expect("submit fixed");
            with_state_mut(|state| {
                let mut chain_state = *state.chain_state.get();
                chain_state.base_fee = 2;
                state.chain_state.set(chain_state);
            });
            let outcome = chain::produce_block(2).expect("execute after base fee change");
            assert_eq!(outcome.block.tx_ids, vec![fixed, dynamic]);
            assert_eq!(chain::get_receipt(&fixed).unwrap().effective_gas_price, 4);
            assert_eq!(chain::get_receipt(&dynamic).unwrap().effective_gas_price, 3);
        })
        .join()
        .unwrap();
    }
}
