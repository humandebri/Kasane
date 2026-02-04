//! どこで: Phase1テスト / 何を: system tx注入が外部tx列挙へ混ざらないことを確認 / なぜ: PR3の内部実行境界を固定するため

use evm_core::chain;
use evm_db::chain_data::{L1BlockInfoParamsV1, L1BlockInfoSnapshotV1};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_code_key};
use evm_db::types::values::{AccountVal, CodeVal};

#[test]
fn injected_system_tx_is_internal_only() {
    init_stable_state();
    with_state_mut(|state| {
        let _ = state.l1_block_info_params.set(L1BlockInfoParamsV1 {
            schema_version: 1,
            spec_id: 101,
            empty_ecotone_scalars: false,
            l1_fee_overhead: 0,
            l1_base_fee_scalar: 1_000_000,
            l1_blob_base_fee_scalar: 0,
            operator_fee_scalar: 0,
            operator_fee_constant: 0,
        });
        let _ = state.l1_block_info_snapshot.set(L1BlockInfoSnapshotV1 {
            schema_version: 1,
            enabled: true,
            l1_block_number: 1,
            l1_base_fee: 1,
            l1_blob_base_fee: 0,
        });
    });

    let caller_principal = vec![0x11];
    let caller = evm_core::hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller, 1_000_000_000_000_000_000).expect("mint");
    let target = [0x44u8; 20];
    install_contract(target, &[0x00]); // STOP

    let out = chain::execute_ic_tx(caller_principal, vec![0xaa], build_ic_tx_bytes(target))
        .expect("execute");
    let block = chain::get_block(out.block_number).expect("block");
    assert_eq!(block.tx_ids.len(), 1);
    assert_eq!(block.tx_ids[0], out.tx_id);
    let (receipt_count, tx_index_count) = with_state(|state| (state.receipts.len(), state.tx_index.len()));
    assert_eq!(receipt_count, 1);
    assert_eq!(tx_index_count, 1);
}

#[test]
fn disabled_snapshot_skips_system_tx_and_keeps_user_indexing() {
    init_stable_state();
    with_state_mut(|state| {
        let _ = state.l1_block_info_params.set(L1BlockInfoParamsV1 {
            schema_version: 1,
            spec_id: 101,
            empty_ecotone_scalars: false,
            l1_fee_overhead: 0,
            l1_base_fee_scalar: 1_000_000,
            l1_blob_base_fee_scalar: 0,
            operator_fee_scalar: 0,
            operator_fee_constant: 0,
        });
        let _ = state.l1_block_info_snapshot.set(L1BlockInfoSnapshotV1 {
            schema_version: 1,
            enabled: false,
            l1_block_number: 1,
            l1_base_fee: 1,
            l1_blob_base_fee: 0,
        });
    });
    let caller_principal = vec![0x21];
    let caller = evm_core::hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller, 1_000_000_000_000_000_000).expect("mint");
    let target = [0x55u8; 20];
    install_contract(target, &[0x00]);
    let out = chain::execute_ic_tx(caller_principal, vec![0xaa], build_ic_tx_bytes(target))
        .expect("execute");
    let block = chain::get_block(out.block_number).expect("block");
    assert_eq!(block.tx_ids.len(), 1);
    let (receipt_count, tx_index_count) = with_state(|state| (state.receipts.len(), state.tx_index.len()));
    assert_eq!(receipt_count, 1);
    assert_eq!(tx_index_count, 1);
}

fn build_ic_tx_bytes(to: [u8; 20]) -> Vec<u8> {
    let value = [0u8; 32];
    let gas_limit = 50_000u64.to_be_bytes();
    let nonce = 0u64.to_be_bytes();
    let max_fee = 2_000_000_000u128.to_be_bytes();
    let max_priority = 1_000_000_000u128.to_be_bytes();
    let data: Vec<u8> = Vec::new();
    let data_len = 0u32.to_be_bytes();
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
        state
            .accounts
            .insert(make_account_key(address), AccountVal::from_parts(0, [0u8; 32], code_hash));
        state.codes.insert(make_code_key(code_hash), CodeVal(code.to_vec()));
    });
}
