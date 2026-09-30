use evm_core::revm_exec::EVM_SPEC_ID;
use revm::{
    context::journal::{JournalEntry, JournalInner},
    database::InMemoryDB,
    primitives::{Address, Log, U256},
    state::{Account, AccountInfo, EvmStorageSlot},
};
use std::fmt::Write;

fn observe(
    out: &mut String,
    label: &str,
    journal: &JournalInner<JournalEntry>,
    source: Address,
    target: Address,
) {
    let account = &journal.state[&source];
    let log_markers = journal
        .logs
        .iter()
        .map(|log| {
            assert_eq!(log.address, source);
            assert!(log.data.topics().is_empty());
            assert_eq!(log.data.data.len(), 1);
            log.data.data[0].to_string()
        })
        .collect::<Vec<_>>()
        .join(",");
    let slot = |key: u64| {
        account
            .storage
            .get(&U256::from(key))
            .map(|s| s.present_value)
            .unwrap_or_default()
    };
    writeln!(
        out,
        "{label} {} {} {} {} {} {} [{log_markers}]",
        slot(0),
        slot(1),
        account.info.balance,
        journal.state[&target].info.balance,
        journal.logs.len(),
        journal.depth
    )
    .unwrap();
}

#[test]
fn journal_nested_commit_and_revert_vectors() {
    let source = Address::with_last_byte(0x11);
    let target = Address::with_last_byte(0x22);
    let mut out = String::new();
    for old in [0u64, 7, 255] {
        for amount in [0u64, 1, 9] {
            let mut journal = JournalInner::<JournalEntry>::new();
            journal.spec = EVM_SPEC_ID;
            let mut source_account = Account::from(AccountInfo {
                balance: U256::from(100),
                nonce: 1,
                ..Default::default()
            });
            source_account
                .storage
                .insert(U256::ZERO, EvmStorageSlot::new(U256::from(old), 0));
            source_account
                .storage
                .insert(U256::from(1), EvmStorageSlot::new(U256::ZERO, 0));
            journal.state.insert(source, source_account);
            journal.state.insert(
                target,
                Account::from(AccountInfo {
                    balance: U256::from(20),
                    nonce: 1,
                    ..Default::default()
                }),
            );
            let mut db = InMemoryDB::default();
            writeln!(out, "case {old} {amount}").unwrap();
            let parent = journal.checkpoint();
            journal
                .sstore(&mut db, source, U256::ZERO, U256::from(11), false)
                .unwrap();
            assert_eq!(
                journal.transfer_loaded(source, target, U256::from(amount)),
                None
            );
            journal.log(Log::new_unchecked(source, vec![], vec![1].into()));
            observe(&mut out, "parent", &journal, source, target);
            let child = journal.checkpoint();
            journal
                .sstore(&mut db, source, U256::ZERO, U256::from(22), false)
                .unwrap();
            journal
                .sstore(&mut db, source, U256::from(1), U256::from(33), false)
                .unwrap();
            assert_eq!(journal.transfer_loaded(source, target, U256::from(3)), None);
            journal.log(Log::new_unchecked(source, vec![], vec![2].into()));
            observe(&mut out, "child", &journal, source, target);
            journal.checkpoint_revert(child);
            observe(&mut out, "child_revert", &journal, source, target);
            let _retry = journal.checkpoint();
            journal
                .sstore(&mut db, source, U256::ZERO, U256::from(44), false)
                .unwrap();
            assert_eq!(journal.transfer_loaded(source, target, U256::from(2)), None);
            journal.log(Log::new_unchecked(source, vec![], vec![3].into()));
            let retained = journal.journal.len();
            journal.checkpoint_commit();
            assert_eq!(journal.journal.len(), retained);
            observe(&mut out, "child_commit", &journal, source, target);
            journal.checkpoint_revert(parent);
            observe(&mut out, "parent_revert", &journal, source, target);
            assert_eq!(journal.state[&source].info.balance, U256::from(100));
            assert_eq!(journal.state[&target].info.balance, U256::from(20));
            assert_eq!(
                journal.state[&source].storage[&U256::ZERO].present_value,
                U256::from(old)
            );
            assert_eq!(
                journal.state[&source].storage[&U256::from(1)].present_value,
                U256::ZERO
            );
            assert!(journal.logs.is_empty());
            assert!(journal.journal.is_empty());
            assert_eq!(journal.depth, 0);
        }
    }
    if let Some(path) = std::env::var_os("KASANE_JOURNAL_VECTORS") {
        std::fs::write(path, out).unwrap();
    }
}
