//! どこで: Phase1テスト / 何を: L1 fee文脈の境界適用とフォールバック計測 / なぜ: PR3仕様を固定するため

use evm_core::chain;
use evm_db::chain_data::{L1BlockInfoParamsV1, L1BlockInfoSnapshotV1};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_code_key};
use evm_db::types::values::{AccountVal, CodeVal};

#[test]
fn snapshot_update_applies_from_next_block() {
    init_stable_state();
    configure_l1_params();
    set_snapshot(true, 1, 100);

    let caller_principal = vec![0x42];
    let caller = evm_core::hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller, 1_000_000_000_000_000_000).expect("mint");
    let target = [0x10u8; 20];
    install_contract(target, &[0x00]); // STOP

    let first = chain::execute_ic_tx(
        caller_principal.clone(),
        vec![0xaa],
        build_ic_tx_bytes(target, 0),
    )
    .expect("execute #1");
    let first_receipt = chain::get_receipt(&first.tx_id).expect("receipt #1");

    set_snapshot(true, 2, 1_000);
    let second = chain::execute_ic_tx(
        caller_principal,
        vec![0xbb],
        build_ic_tx_bytes(target, 1),
    )
    .expect("execute #2");
    let second_receipt = chain::get_receipt(&second.tx_id).expect("receipt #2");

    assert_eq!(first.status, second.status);
    assert_eq!(first_receipt.gas_used, second_receipt.gas_used);
    assert!(second_receipt.l1_data_fee > first_receipt.l1_data_fee);
    assert_eq!(
        first_receipt.total_fee,
        u128::from(first_receipt.gas_used)
            .saturating_mul(u128::from(first_receipt.effective_gas_price))
            .saturating_add(first_receipt.l1_data_fee)
            .saturating_add(first_receipt.operator_fee)
    );
}

#[test]
fn snapshot_disabled_records_fallback_metric() {
    init_stable_state();
    configure_l1_params();
    set_snapshot(false, 0, 0);

    let caller_principal = vec![0x99];
    let caller = evm_core::hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller, 1_000_000_000_000_000_000).expect("mint");
    let target = [0x22u8; 20];
    install_contract(target, &[0x00]); // STOP

    let _ = chain::execute_ic_tx(caller_principal, vec![0xaa], build_ic_tx_bytes(target, 0))
        .expect("execute");
    let fallback_count = with_state(|state| state.ops_state.get().l1_fee_fallback_count);
    assert_eq!(fallback_count, 1);
}

#[test]
fn isthmus_operator_fee_uses_gas_used_not_gas_limit() {
    init_stable_state();
    configure_l1_params_custom(107, 1_000_000, 7);
    set_snapshot(true, 1, 100);

    let caller_principal = vec![0x77];
    let caller = evm_core::hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller, 1_000_000_000_000_000_000).expect("mint");
    let target = [0x33u8; 20];
    install_contract(target, &[0x00]); // STOP

    let out = chain::execute_ic_tx(caller_principal, vec![0xaa], build_ic_tx_bytes(target, 0))
        .expect("execute");
    let receipt = chain::get_receipt(&out.tx_id).expect("receipt");

    let gas_limit = 50_000u128;
    let expected_operator_fee = u128::from(receipt.gas_used).saturating_add(7);
    assert_eq!(receipt.operator_fee, expected_operator_fee);
    assert_ne!(receipt.operator_fee, gas_limit.saturating_add(7));
    assert_eq!(
        receipt.total_fee,
        u128::from(receipt.gas_used)
            .saturating_mul(u128::from(receipt.effective_gas_price))
            .saturating_add(receipt.l1_data_fee)
            .saturating_add(receipt.operator_fee)
    );
}

fn configure_l1_params() {
    configure_l1_params_custom(101, 0, 0);
}

fn configure_l1_params_custom(spec_id: u8, operator_fee_scalar: u128, operator_fee_constant: u128) {
    with_state_mut(|state| {
        let _ = state.l1_block_info_params.set(L1BlockInfoParamsV1 {
            schema_version: 1,
            spec_id,
            empty_ecotone_scalars: false,
            l1_fee_overhead: 0,
            l1_base_fee_scalar: 1_000_000,
            l1_blob_base_fee_scalar: 0,
            operator_fee_scalar,
            operator_fee_constant,
        });
    });
}

fn set_snapshot(enabled: bool, l1_block_number: u64, l1_base_fee: u128) {
    with_state_mut(|state| {
        let _ = state.l1_block_info_snapshot.set(L1BlockInfoSnapshotV1 {
            schema_version: 1,
            enabled,
            l1_block_number,
            l1_base_fee,
            l1_blob_base_fee: 0,
        });
    });
}

fn build_ic_tx_bytes(to: [u8; 20], nonce: u64) -> Vec<u8> {
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

fn install_contract(address: [u8; 20], code: &[u8]) {
    let code_hash = evm_core::hash::keccak256(code);
    with_state_mut(|state| {
        let account_key = make_account_key(address);
        let account = AccountVal::from_parts(0, [0u8; 32], code_hash);
        let code_key = make_code_key(code_hash);
        state.accounts.insert(account_key, account);
        state.codes.insert(code_key, CodeVal(code.to_vec()));
    });
}
