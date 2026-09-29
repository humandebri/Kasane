use evm_core::{
    revm_db::RevmStableDb,
    revm_exec::{execute_tx, BlockExecContext, ExecPath},
};
use evm_db::{
    chain_data::{TxId, TxKind},
    stable_state::init_stable_state,
};
use revm::{
    context::TxEnv,
    primitives::{Address, Bytes, HashMap, TxKind as RevmTxKind, U256},
    state::{Account, AccountInfo, Bytecode, EvmStorageSlot},
    Database, DatabaseCommit,
};

fn account(balance: u64, nonce: u64, code: Vec<u8>, slot: u64) -> Account {
    let code = Bytecode::new_raw(Bytes::from(code));
    let mut account = Account::from(AccountInfo {
        balance: U256::from(balance),
        nonce,
        code_hash: code.hash_slow(),
        code: Some(code),
        ..Default::default()
    });
    account.mark_touch();
    account.storage.insert(
        U256::ZERO,
        EvmStorageSlot::new_changed(U256::ZERO, U256::from(slot), 0),
    );
    account
}

#[test]
fn kasane_nested_call_revert_preserves_transaction_accounting() {
    let sender = Address::with_last_byte(0x11);
    let parent = Address::with_last_byte(0x22);
    let child = Address::with_last_byte(0x33);
    for child_exit in [0x00, 0xfd, 0xfe] {
        // STOP, REVERT, exceptional halt
        for parent_reverts in [false, true] {
            for priority in [None, Some(1)] {
                // Stable memory is thread-local; init_stable_state reopens it rather
                // than clearing it. Each case therefore needs a fresh thread.
                std::thread::spawn(move || {
                    init_stable_state();
                    // Child writes slot 0, emits a log, then exits.
                    let mut child_code = hex::decode("601660005560006000a0").unwrap();
                    if child_exit == 0xfd {
                        child_code.extend([0x60, 0, 0x60, 0]);
                    }
                    child_code.push(child_exit);
                    // Parent writes and logs, calls child with value=3, ignores CALL's
                    // success bit, then writes slot 1 before its own exit.
                    let mut parent_code =
                        hex::decode("600b60005560006000a06000600060006000600373").unwrap();
                    parent_code.extend(child.as_slice());
                    // Bound the child's gas so even an exceptional halt leaves parent gas.
                    parent_code.extend(hex::decode("61fffff150602c600155").unwrap());
                    if parent_reverts {
                        parent_code.extend(hex::decode("60006000fd").unwrap());
                    } else {
                        parent_code.push(0);
                    }
                    let mut db = RevmStableDb;
                    db.commit(HashMap::from_iter([
                        (sender, account(1_000_000_000, 7, vec![], 0)),
                        (parent, account(100, 1, parent_code, 7)),
                        (child, account(20, 1, child_code, 8)),
                    ]));
                    let tx = TxEnv::builder()
                        .caller(sender)
                        .nonce(7)
                        .chain_id(Some(evm_db::chain_data::constants::CHAIN_ID))
                        .kind(RevmTxKind::Call(parent))
                        .gas_limit(300_000)
                        .gas_price(3)
                        .gas_priority_fee(priority)
                        .value(U256::from(5))
                        .build()
                        .unwrap();
                    let outcome = execute_tx(
                        TxId([0x42; 32]),
                        0,
                        TxKind::EthSigned,
                        &[],
                        tx,
                        &BlockExecContext {
                            block_number: 1,
                            timestamp: 1,
                            base_fee: 1,
                            block_gas_limit: 1_000_000,
                        },
                        ExecPath::UserTx,
                    )
                    .unwrap();
                    let success = !parent_reverts;
                    let child_success = success && child_exit == 0;
                    let expected_log_addresses = if child_success {
                        vec![parent, child]
                    } else if success {
                        vec![parent]
                    } else {
                        vec![]
                    };
                    assert_eq!(
                        outcome
                            .receipt
                            .logs
                            .iter()
                            .map(|log| log.address)
                            .collect::<Vec<_>>(),
                        expected_log_addresses
                    );
                    assert_eq!(outcome.receipt.status, u8::from(success));
                    assert_eq!(
                        outcome.receipt.logs.len(),
                        if success {
                            1 + usize::from(child_success)
                        } else {
                            0
                        }
                    );
                    assert_eq!(
                        db.storage(parent, U256::ZERO).unwrap(),
                        U256::from(if success { 11 } else { 7 })
                    );
                    assert_eq!(
                        db.storage(parent, U256::from(1)).unwrap(),
                        U256::from(if success { 44 } else { 0 })
                    );
                    assert_eq!(
                        db.storage(child, U256::ZERO).unwrap(),
                        U256::from(if child_success { 22 } else { 8 })
                    );
                    let effective_price = if priority.is_some() { 2 } else { 3 };
                    assert_eq!(outcome.receipt.effective_gas_price, effective_price);
                    let fee = u128::from(outcome.receipt.gas_used) * u128::from(effective_price);
                    assert!(fee > 0);
                    assert_eq!(outcome.receipt.total_fee, fee);
                    let sender_info = db.basic(sender).unwrap().unwrap();
                    assert_eq!(sender_info.nonce, 8);
                    assert_eq!(
                        sender_info.balance,
                        U256::from(1_000_000_000u128 - fee - if success { 5 } else { 0 })
                    );
                    assert_eq!(
                        db.basic(parent).unwrap().unwrap().balance,
                        U256::from(
                            100 + if success { 5 } else { 0 } - if child_success { 3 } else { 0 }
                        )
                    );
                    assert_eq!(
                        db.basic(child).unwrap().unwrap().balance,
                        U256::from(20 + if child_success { 3 } else { 0 })
                    );
                    assert_eq!(
                        db.basic(Address::from(evm_core::fee_recipient()))
                            .unwrap()
                            .unwrap()
                            .balance,
                        U256::from(fee)
                    );
                })
                .join()
                .unwrap();
            }
        }
    }
}
