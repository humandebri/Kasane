use evm_core::{
    revm_db::RevmStableDb,
    revm_exec::{execute_tx, BlockExecContext, ExecOutcome, ExecPath},
};
use evm_db::{
    chain_data::{TxId, TxKind},
    stable_state::{current_evm_state_epoch, init_stable_state, with_state, with_state_mut},
    types::{keys::make_account_key, values::AccountVal},
};
use revm::{
    context::TxEnv,
    primitives::{Address, Bytes, HashMap, TxKind as RevmTxKind, U256},
    state::{Account, AccountInfo, Bytecode, EvmStorageSlot},
    Database, DatabaseCommit, DatabaseRef,
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

fn execute_test_call(parent: Address) -> ExecOutcome {
    let tx = TxEnv::builder()
        .caller(Address::with_last_byte(0x11))
        .nonce(7)
        .chain_id(Some(evm_db::chain_data::constants::CHAIN_ID))
        .kind(RevmTxKind::Call(parent))
        .gas_limit(500_000)
        .gas_price(3)
        .gas_priority_fee(Some(1))
        .build()
        .unwrap();
    execute_tx(
        TxId([0x43; 32]),
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
    .unwrap()
}

fn assert_transaction_accounting(db: &mut RevmStableDb, outcome: &ExecOutcome) {
    let fee = u128::from(outcome.receipt.gas_used) * 2;
    assert_eq!(outcome.receipt.effective_gas_price, 2);
    assert_eq!(outcome.receipt.total_fee, fee);
    let sender = db.basic(Address::with_last_byte(0x11)).unwrap().unwrap();
    assert_eq!(sender.nonce, 8);
    assert_eq!(sender.balance, U256::from(1_000_000_000u128 - fee));
    assert_eq!(
        db.basic(Address::from(evm_core::fee_recipient()))
            .unwrap()
            .unwrap()
            .balance,
        U256::from(fee)
    );
}

#[test]
fn untouched_revm_account_does_not_create_stable_state() {
    std::thread::spawn(|| {
        init_stable_state();
        let address = Address::with_last_byte(0x71);
        let mut account = Account::from(AccountInfo {
            balance: U256::from(7),
            nonce: 1,
            ..Default::default()
        });
        assert!(!account.is_touched());
        let mut db = RevmStableDb;
        let epoch = current_evm_state_epoch();
        db.commit(HashMap::from_iter([(address, account.clone())]));
        assert!(db.basic(address).unwrap().is_none());
        assert_eq!(current_evm_state_epoch(), epoch);

        account.mark_touch();
        db.commit(HashMap::from_iter([(address, account)]));
        assert_eq!(db.basic(address).unwrap().unwrap().balance, U256::from(7));
        assert!(current_evm_state_epoch() > epoch);
    })
    .join()
    .unwrap();
}

#[test]
fn legacy_empty_record_is_absent_from_revm_database_reads() {
    std::thread::spawn(|| {
        init_stable_state();
        let address = Address::with_last_byte(0x72);
        let key = make_account_key(address.into_array());
        with_state_mut(|state| {
            state
                .accounts
                .insert(key, AccountVal::from_parts(0, [0u8; 32], [0u8; 32]));
        });
        assert!(with_state(|state| state.accounts.get(&key).is_some()));
        let mut db = RevmStableDb;
        assert!(db.basic(address).unwrap().is_none());
        assert!(db.basic_ref(address).unwrap().is_none());

        with_state_mut(|state| {
            state.accounts.insert(
                key,
                AccountVal::from_parts(0, [0u8; 32], revm::primitives::KECCAK_EMPTY.0),
            );
        });
        assert!(db.basic(address).unwrap().is_none());
        assert!(db.basic_ref(address).unwrap().is_none());

        with_state_mut(|state| {
            state
                .accounts
                .insert(key, AccountVal::from_parts(1, [0u8; 32], [0u8; 32]));
        });
        assert_eq!(db.basic(address).unwrap().unwrap().nonce, 1);
        assert_eq!(db.basic_ref(address).unwrap().unwrap().nonce, 1);
    })
    .join()
    .unwrap();
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
                        vec![parent.into_array(), child.into_array()]
                    } else if success {
                        vec![parent.into_array()]
                    } else {
                        vec![]
                    };
                    assert_eq!(
                        outcome
                            .receipt
                            .logs
                            .iter()
                            .map(|log| log.address.into_array())
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

#[test]
fn create_is_persisted_only_when_parent_commits() {
    let sender = Address::with_last_byte(0x11);
    let parent = Address::with_last_byte(0x52);
    let created = parent.create(1);
    for parent_reverts in [false, true] {
        for legacy_empty_created in [false, true] {
            std::thread::spawn(move || {
                init_stable_state();
                if legacy_empty_created {
                    with_state_mut(|state| {
                        state.accounts.insert(
                            make_account_key(created.into_array()),
                            AccountVal::from_parts(0, [0u8; 32], [0u8; 32]),
                        );
                    });
                }
                // Return one JUMPDEST byte from the init code so creation is observable.
                let mut code = vec![
                    0x69, 0x60, 0x5b, 0x60, 0x00, 0x53, 0x60, 0x01, 0x60, 0x00, 0xf3, 0x60, 0x00,
                    0x52, 0x60, 0x0a, 0x60, 0x16, 0x60, 0x00, 0xf0, 0x50,
                ];
                code.extend(if parent_reverts {
                    [0x60, 0x00, 0x60, 0x00, 0xfd]
                } else {
                    [0x60, 0x00, 0x60, 0x00, 0x00]
                });
                let mut db = RevmStableDb;
                db.commit(HashMap::from_iter([
                    (sender, account(1_000_000_000, 7, vec![], 0)),
                    (parent, account(100, 1, code, 7)),
                ]));
                let outcome = execute_test_call(parent);
                assert_eq!(outcome.receipt.status, u8::from(!parent_reverts));
                assert!(outcome.receipt.logs.is_empty());
                assert_eq!(
                    db.basic(parent).unwrap().unwrap().nonce,
                    if parent_reverts { 1 } else { 2 }
                );
                let created_info = db.basic(created).unwrap();
                assert_eq!(created_info.is_some(), !parent_reverts);
                if !parent_reverts {
                    assert_eq!(
                        created_info.unwrap().code_hash,
                        Bytecode::new_raw(Bytes::from_static(&[0x5b])).hash_slow(),
                        "legacy_empty_created={legacy_empty_created}"
                    );
                } else if legacy_empty_created {
                    assert!(with_state(|state| state
                        .accounts
                        .get(&make_account_key(created.into_array()))
                        .is_some()));
                }
                assert_eq!(db.storage(parent, U256::ZERO).unwrap(), U256::from(7));
                assert_transaction_accounting(&mut db, &outcome);
            })
            .join()
            .unwrap();
        }
    }
}

#[test]
fn selfdestruct_balance_transfer_rolls_back_with_parent() {
    let sender = Address::with_last_byte(0x11);
    let parent = Address::with_last_byte(0x62);
    let child = Address::with_last_byte(0x63);
    let beneficiary = Address::with_last_byte(0x64);
    for parent_reverts in [false, true] {
        std::thread::spawn(move || {
            init_stable_state();
            let mut child_code = vec![0x73];
            child_code.extend_from_slice(beneficiary.as_slice());
            child_code.push(0xff);
            let child_code_hash = Bytecode::new_raw(Bytes::from(child_code.clone())).hash_slow();
            // CALL child with value 0, then either STOP or REVERT in the parent.
            let mut parent_code = vec![0x60, 0, 0x60, 0, 0x60, 0, 0x60, 0, 0x60, 0, 0x73];
            parent_code.extend_from_slice(child.as_slice());
            parent_code.extend([0x5a, 0xf1, 0x50]);
            if parent_reverts {
                parent_code.extend([0x60, 0, 0x60, 0, 0xfd]);
            } else {
                parent_code.push(0);
            }
            let mut db = RevmStableDb;
            db.commit(HashMap::from_iter([
                (sender, account(1_000_000_000, 7, vec![], 0)),
                (parent, account(100, 1, parent_code, 7)),
                (child, account(50, 1, child_code, 8)),
                (beneficiary, account(10, 1, vec![], 0)),
            ]));
            let outcome = execute_test_call(parent);
            assert_eq!(outcome.receipt.status, u8::from(!parent_reverts));
            assert!(outcome.receipt.logs.is_empty());
            let child_info = db.basic(child).unwrap().unwrap();
            assert_eq!(
                child_info.balance,
                U256::from(if parent_reverts { 50 } else { 0 })
            );
            // Prague keeps the code of a contract created before this transaction.
            assert_eq!(child_info.code_hash, child_code_hash);
            assert_eq!(db.storage(child, U256::ZERO).unwrap(), U256::from(8));
            assert_eq!(
                db.basic(beneficiary).unwrap().unwrap().balance,
                U256::from(if parent_reverts { 10 } else { 60 })
            );
            assert_transaction_accounting(&mut db, &outcome);
        })
        .join()
        .unwrap();
    }
}
