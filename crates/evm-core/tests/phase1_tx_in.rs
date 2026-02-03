//! どこで: Phase1テスト / 何を: TxIn入口の最小検証 / なぜ: submit経路の統一で退行を防ぐため

use evm_core::chain::{self, ChainError, TxIn};
use evm_db::chain_data::{TxKind, TxLocKind};
use evm_db::stable_state::init_stable_state;

#[test]
fn submit_tx_in_rejects_unsupported_kind() {
    init_stable_state();
    let err = chain::submit_tx_in(TxIn::OpDeposit(vec![0xde, 0xad])).unwrap_err();
    assert_eq!(err, ChainError::UnsupportedTxKind);
}

#[test]
fn submit_tx_in_eth_keeps_existing_decode_rules() {
    init_stable_state();
    let err = chain::submit_tx_in(TxIn::EthSigned(vec![0x02, 0x01, 0x02])).unwrap_err();
    assert_eq!(err, ChainError::DecodeFailed);
}

#[test]
fn submit_tx_in_ic_synthetic_enqueues_tx() {
    init_stable_state();
    let caller_principal = vec![0x42];
    let canister_id = vec![0x99];
    let tx_bytes = build_ic_tx_bytes(0);
    let tx_id = chain::submit_tx_in(TxIn::IcSynthetic {
        caller_principal: caller_principal.clone(),
        canister_id: canister_id.clone(),
        tx_bytes,
    })
    .expect("submit ic tx");

    let envelope = chain::get_tx_envelope(&tx_id).expect("stored tx");
    assert_eq!(envelope.kind, TxKind::IcSynthetic);
    assert_eq!(envelope.caller_principal, caller_principal);
    assert_eq!(envelope.canister_id, canister_id);
    let loc = chain::get_tx_loc(&tx_id).expect("tx location");
    assert_eq!(loc.kind, TxLocKind::Queued);
}

fn build_ic_tx_bytes(nonce: u64) -> Vec<u8> {
    let to = [0x11u8; 20];
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce = nonce.to_be_bytes();
    let max_fee = 2_000_000_000u128.to_be_bytes();
    let max_priority = 1_000_000_000u128.to_be_bytes();
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
