//! どこで: PR0の差分テスト基盤 / 何を: tx結果とblockヘッダ要素のスナップショット固定 / なぜ: 後続PRで意図しない挙動差分を早期検知するため

use evm_core::chain::{self, ChainError};
use evm_core::hash;
use evm_db::chain_data::constants::DROP_CODE_DECODE;
use evm_db::chain_data::{
    Head, ReadyKey, SenderKey, SenderNonceKey, StoredTxBytes, TxId, TxKind, TxLoc, TxLocKind,
};
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
fn snapshot_tx_outcome_matrix_and_block_fields() {
    init_stable_state();
    relax_fee_floor_for_tests();
    // ブロックハッシュを時刻依存で揺らさないため、head.timestamp を固定する。
    with_state_mut(|state| {
        let head = *state.head.get();
        state.head.set(Head {
            number: head.number,
            block_hash: head.block_hash,
            timestamp: 4_000_000_000,
        });
    });
    let caller_principal = vec![0x42];
    let caller = hash::derive_evm_address_from_principal(&caller_principal).expect("must derive");
    common::fund_account(caller, 1_000_000_000_000_000_000);

    let success_target = [0x10u8; 20];
    let revert_target = [0x11u8; 20];
    let halt_target = [0x12u8; 20];
    common::install_contract(success_target, &[0x00]); // STOP
    common::install_contract(revert_target, &[0x60, 0x00, 0x60, 0x00, 0xfd]); // REVERT(0, 0)
    common::install_contract(halt_target, &[0xfe]); // INVALID

    let (_, success) = common::execute_ic_tx_via_produce(
        caller_principal.clone(),
        vec![0xaa],
        common::build_ic_tx_bytes(success_target, 0, 2_000_000_000, 1_000_000_000),
    );
    let (_, revert) = common::execute_ic_tx_via_produce(
        caller_principal.clone(),
        vec![0xbb],
        common::build_ic_tx_bytes(revert_target, 1, 2_000_000_000, 1_000_000_000),
    );
    let (_, halt) = common::execute_ic_tx_via_produce(
        caller_principal,
        vec![0xcc],
        common::build_ic_tx_bytes(halt_target, 2, 2_000_000_000, 1_000_000_000),
    );

    let matrix = format!(
        "tx_statuses=[{}, {}, {}]",
        success.status, revert.status, halt.status
    );

    let block = chain::get_block(3).expect("block #3");

    let block_outcome = format!(
        "number={} block_hash={} tx_list_hash={} state_root={}",
        block.number,
        hex32(block.block_hash),
        hex32(block.tx_list_hash),
        hex32(block.state_root)
    );

    println!("SNAPSHOT_TX_MATRIX: {matrix}");
    println!("SNAPSHOT_BLOCK: {block_outcome}");

    assert_eq!(matrix, "tx_statuses=[1, 0, 0]");
    // 意図差分の履歴:
    // - OP由来のsystem tx会計を除去し、標準EVM実行へ統一したことで state_root/block_hash が更新
    // - fee floor をテスト内で固定したことで block_hash/state_root が再計算された
    assert_eq!(
        block_outcome,
        "number=3 block_hash=fe2bab5f965a7be9d7880d7a9d72394879987b1dfc70f406e306d2fe020c472c tx_list_hash=60e50781adb0b02f798fb14df878b982f864e81f3d2220e86e924a131e213ee0 state_root=4d2ba91fcb5fe0c9ee5a1a29dca4f5850ee13cff0e1e4035f762af2ed4e31620"
    );
}

#[test]
fn snapshot_decode_drop_tuple() {
    init_stable_state();

    let tx_id = TxId([0x10u8; 32]);
    let envelope = StoredTxBytes::new_with_fees(
        tx_id,
        TxKind::EthSigned,
        vec![0x01],
        None,
        Vec::new(),
        Vec::new(),
        0,
        0,
        false,
    );
    let sender = [0x11u8; 20];
    let pending_key = SenderNonceKey::new(sender, 0);
    with_state_mut(|state| {
        state.tx_store.insert(tx_id, envelope);
        state.tx_locs.insert(tx_id, TxLoc::queued(0));
        state.pending_by_sender_nonce.insert(pending_key, tx_id);
        state.pending_meta_by_tx_id.insert(tx_id, pending_key);
        state.pending_min_nonce.insert(SenderKey::new(sender), 0);
        let key = ReadyKey::new(1, 0, 0, tx_id.0);
        state.ready_queue.insert(key, tx_id);
        state.ready_key_by_tx_id.insert(tx_id, key);
    });

    let err = chain::produce_block(1).expect_err("produce_block should fail");
    assert_eq!(err, ChainError::NoExecutableTx);

    let loc = chain::get_tx_loc(&tx_id).expect("tx_loc");
    assert_eq!(loc.kind, TxLocKind::Dropped);
    assert_eq!(loc.drop_code, DROP_CODE_DECODE);
}

fn hex32(value: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in value {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
