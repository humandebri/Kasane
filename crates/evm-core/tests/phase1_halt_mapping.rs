//! どこで: Phase1テスト / 何を: 固定タグのExecError分類を確認 / なぜ: 文字列依存を避けるため

use evm_core::chain::{self, ChainError};
use evm_core::revm_exec::ExecError;
use evm_db::chain_data::{L1BlockInfoParamsV1, L1BlockInfoSnapshotV1};
use evm_db::stable_state::{init_stable_state, with_state_mut};

#[test]
fn invalid_spec_id_is_mapped_as_fixed_error() {
    init_stable_state();
    with_state_mut(|state| {
        let _ = state.l1_block_info_params.set(L1BlockInfoParamsV1 {
            schema_version: 1,
            spec_id: 99,
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

    let caller_principal = vec![0x22];
    let result = chain::execute_ic_tx(
        caller_principal,
        vec![0xaa],
        build_ic_tx_bytes([0x10u8; 20]),
    );
    assert_eq!(
        result,
        Err(ChainError::ExecFailed(Some(ExecError::InvalidL1SpecId(99))))
    );
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
