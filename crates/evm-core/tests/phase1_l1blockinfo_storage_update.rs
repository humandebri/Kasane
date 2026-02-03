//! どこで: Phase1テスト / 何を: L1BlockInfo system tx のstorage更新を検証 / なぜ: 注入の実効性を担保するため

use evm_core::chain;
use evm_core::hash;
use evm_db::chain_data::{L1BlockInfoParamsV1, L1BlockInfoSnapshotV1};
use evm_db::stable_state::{init_stable_state, with_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_code_key, make_storage_key};
use evm_db::types::values::{AccountVal, CodeVal, U256Val};
use op_revm::constants::L1_BLOCK_CONTRACT;
use revm::primitives::U256;

#[test]
fn enabled_snapshot_updates_l1_storage_v1_layout() {
    init_stable_state();
    install_l1block_mock_runtime();
    configure_l1(101, true, 777, 0);
    run_single_user_tx(0x61);

    let slot1 = read_storage_word(1);
    assert_eq!(slot1, U256::from(777u64));
}

#[test]
fn enabled_snapshot_updates_l1_storage_v2_layout() {
    init_stable_state();
    install_l1block_mock_runtime();
    configure_l1(103, true, 888, 222);
    run_single_user_tx(0x62);

    let slot2 = read_storage_word(2);
    assert_eq!(slot2, U256::from(888u64));
}

#[test]
fn disabled_snapshot_skips_l1_storage_update() {
    init_stable_state();
    install_l1block_mock_runtime();
    configure_l1(101, false, 999, 0);
    run_single_user_tx(0x63);

    assert_eq!(read_storage_word(1), U256::ZERO);
    assert_eq!(read_storage_word(2), U256::ZERO);
}

fn configure_l1(spec_id: u8, enabled: bool, l1_base_fee: u128, l1_blob_base_fee: u128) {
    with_state_mut(|state| {
        let _ = state.l1_block_info_params.set(L1BlockInfoParamsV1 {
            schema_version: 1,
            spec_id,
            empty_ecotone_scalars: false,
            l1_fee_overhead: 7,
            l1_base_fee_scalar: 1_000_000,
            l1_blob_base_fee_scalar: 5,
            operator_fee_scalar: 0,
            operator_fee_constant: 0,
        });
        let _ = state.l1_block_info_snapshot.set(L1BlockInfoSnapshotV1 {
            schema_version: 1,
            enabled,
            l2_block_number: 1,
            l1_base_fee,
            l1_blob_base_fee,
        });
    });
}

fn run_single_user_tx(tag: u8) {
    let caller_principal = vec![tag];
    let caller = hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller, 1_000_000_000_000_000_000).expect("mint");
    let target = [tag; 20];
    install_user_stop_contract(target);
    chain::execute_ic_tx(caller_principal, vec![0xaa], build_ic_tx_bytes(target)).expect("execute");
}

fn install_user_stop_contract(address: [u8; 20]) {
    let code = vec![0x00u8];
    let code_hash = hash::keccak256(&code);
    with_state_mut(|state| {
        state
            .accounts
            .insert(make_account_key(address), AccountVal::from_parts(0, [0u8; 32], code_hash));
        state.codes.insert(make_code_key(code_hash), CodeVal(code));
    });
}

fn install_l1block_mock_runtime() {
    // slot0 = calldataload(0), slot1 = calldataload(0x44), slot2 = calldataload(0xa4)
    let code = hex_to_bytes("60003560005560443560015560a43560025500");
    let code_hash = hash::keccak256(&code);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(L1_BLOCK_CONTRACT.as_ref());
    with_state_mut(|state| {
        state
            .accounts
            .insert(make_account_key(addr), AccountVal::from_parts(0, [0u8; 32], code_hash));
        state.codes.insert(make_code_key(code_hash), CodeVal(code));
    });
}

fn read_storage_word(slot_u64: u64) -> U256 {
    let mut addr = [0u8; 20];
    addr.copy_from_slice(L1_BLOCK_CONTRACT.as_ref());
    with_state(|state| {
        let key = make_storage_key(addr, U256::from(slot_u64).to_be_bytes());
        state
            .storage
            .get(&key)
            .map(|U256Val(v)| U256::from_be_bytes(v))
            .unwrap_or(U256::ZERO)
    })
}

fn build_ic_tx_bytes(to: [u8; 20]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(2u8);
    out.extend_from_slice(&to);
    out.extend_from_slice(&[0u8; 32]);
    out.extend_from_slice(&50_000u64.to_be_bytes());
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&2_000_000_000u128.to_be_bytes());
    out.extend_from_slice(&1_000_000_000u128.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

fn hex_to_bytes(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 2);
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        let hi = hex_nibble(bytes[i]);
        let lo = hex_nibble(bytes[i + 1]);
        out.push((hi << 4) | lo);
        i += 2;
    }
    out
}

fn hex_nibble(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}
