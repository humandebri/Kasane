//! どこで: PR0の差分テスト基盤 / 何を: tx結果とblockヘッダ要素のスナップショット固定 / なぜ: 後続PRで意図しない挙動差分を早期検知するため

use evm_core::chain::{self, ChainError};
use evm_core::hash;
use evm_db::chain_data::constants::{DROP_CODE_DECODE, DROP_CODE_INVALID_FEE};
use evm_db::chain_data::{ReadyKey, SenderKey, SenderNonceKey, StoredTxBytes, TxId, TxKind, TxLoc, TxLocKind};
use evm_db::stable_state::{init_stable_state, with_state_mut};
use evm_db::types::keys::{make_account_key, make_code_key};
use evm_db::types::values::{AccountVal, CodeVal};

#[test]
fn snapshot_tx_outcome_matrix_and_block_fields() {
    init_stable_state();
    let caller_principal = vec![0x42];
    let caller = hash::caller_evm_from_principal(&caller_principal);
    chain::dev_mint(caller, 1_000_000_000_000_000_000).expect("mint");

    let success_target = [0x10u8; 20];
    let revert_target = [0x11u8; 20];
    let halt_target = [0x12u8; 20];
    install_contract(success_target, &[0x00]); // STOP
    install_contract(revert_target, &[0x60, 0x00, 0x60, 0x00, 0xfd]); // REVERT(0, 0)
    install_contract(halt_target, &[0xfe]); // INVALID

    let success = chain::execute_ic_tx(
        caller_principal.clone(),
        vec![0xaa],
        build_ic_tx_bytes(success_target, 0),
    )
    .expect("execute success");
    let revert = chain::execute_ic_tx(
        caller_principal.clone(),
        vec![0xbb],
        build_ic_tx_bytes(revert_target, 1),
    )
    .expect("execute revert");
    let halt = chain::execute_ic_tx(
        caller_principal,
        vec![0xcc],
        build_ic_tx_bytes(halt_target, 2),
    )
    .expect("execute halt");

    let matrix = format!(
        "tx_statuses=[{}, {}, {}] final_statuses=[{}, {}, {}]",
        success.status, revert.status, halt.status, success.final_status, revert.final_status, halt.final_status
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

    assert_eq!(
        matrix,
        "tx_statuses=[1, 0, 0] final_statuses=[Success, Revert, Halt:InvalidOpcode]"
    );
    // 意図差分の履歴:
    // - OP由来のsystem tx会計を除去し、標準EVM実行へ統一したことで state_root/block_hash が更新
    assert_eq!(
        block_outcome,
        "number=3 block_hash=9148f185b1ba2c03f961cce08357f3193930e5cd6e39867fef2160d7da4014ce tx_list_hash=4ad087ec0641a22f03bb82cb8cf391aca8c73cb30fd8eeda10b813d1f2a6c6df state_root=93c38df78b09ca12737cd5a446baa0a24444c2d799c7bb633ab6ccb673af1217"
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
    assert_eq!(loc.kind, TxLocKind::Queued);
    assert_ne!(loc.drop_code, DROP_CODE_DECODE);
    assert_ne!(loc.drop_code, DROP_CODE_INVALID_FEE);
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
    let code_hash = hash::keccak256(code);
    with_state_mut(|state| {
        let account_key = make_account_key(address);
        let account = AccountVal::from_parts(0, [0u8; 32], code_hash);
        let code_key = make_code_key(code_hash);
        state.accounts.insert(account_key, account);
        state.codes.insert(code_key, CodeVal(code.to_vec()));
    });
}

fn hex32(value: [u8; 32]) -> String {
    let mut out = String::with_capacity(64);
    for byte in value {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
