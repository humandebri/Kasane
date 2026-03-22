//! どこで: state_root のテスト分離ファイル / 何を: NodeDB と journal 周辺の回帰確認 / なぜ: 本体から test-only 実装を外すため

use super::*;
use evm_db::chain_data::NodeRecord;
use evm_db::stable_state::{init_stable_state, with_state_mut, StableState};
use evm_db::types::keys::make_storage_key;
use evm_db::types::values::U256Val;
use std::collections::{BTreeMap, BTreeSet};

fn apply_node_db_records(state: &mut StableState, records: Vec<(HashKey, NodeRecord)>) {
    let mut next: BTreeMap<HashKey, NodeRecord> = BTreeMap::new();
    for (key, record) in records {
        if keccak256(&record.rlp) != key.0 {
            continue;
        }
        next.insert(key, record);
    }
    let mut counts: NodeDeltaCounts = BTreeMap::new();
    let mut new_records: NewNodeRecords = BTreeMap::new();
    let mut all_keys: BTreeSet<HashKey> = BTreeSet::new();
    for key in state.state_root_node_db.iter().map(|entry| *entry.key()) {
        all_keys.insert(key);
    }
    for key in next.keys().copied() {
        all_keys.insert(key);
    }
    for key in all_keys {
        let before = state
            .state_root_node_db
            .get(&key)
            .map(|value| i64::from(value.refcnt))
            .unwrap_or(0);
        let after = next
            .get(&key)
            .map(|value| i64::from(value.refcnt))
            .unwrap_or(0);
        let diff = after - before;
        if diff != 0 {
            counts.insert(key, diff);
        }
        if let Some(record) = next.get(&key) {
            new_records.insert(key, record.rlp.clone());
        }
    }
    apply_journal(
        state,
        JournalUpdate {
            node_delta_counts: counts,
            new_node_records: new_records,
            anchor_delta: AnchorDelta::default(),
        },
    );
}

#[test]
fn node_db_refcnt_and_gc_follow_records() {
    init_stable_state();
    with_state_mut(|state| {
        let rlp1 = vec![0x80];
        let rlp2 = vec![0x81];
        let k1 = HashKey(keccak256(&rlp1));
        let k2 = HashKey(keccak256(&rlp2));

        apply_node_db_records(
            state,
            vec![
                (k1, NodeRecord::new(2, rlp1.clone())),
                (k2, NodeRecord::new(1, rlp2.clone())),
            ],
        );
        assert_eq!(state.state_root_node_db.len(), 2);
        assert_eq!(state.state_root_node_db.get(&k1).map(|r| r.refcnt), Some(2));
        assert_eq!(state.state_root_node_db.get(&k2).map(|r| r.refcnt), Some(1));
        assert_eq!(state.state_root_metrics.get().node_db_reachable, 2);
        assert_eq!(state.state_root_metrics.get().node_db_unreachable, 0);

        apply_node_db_records(state, vec![(k1, NodeRecord::new(1, rlp1))]);
        assert_eq!(state.state_root_node_db.len(), 1);
        assert_eq!(state.state_root_node_db.get(&k1).map(|r| r.refcnt), Some(1));
        assert!(state.state_root_node_db.get(&k2).is_none());
        assert_eq!(state.state_root_metrics.get().node_db_reachable, 1);
        assert_eq!(state.state_root_metrics.get().node_db_unreachable, 0);
    });
}

#[test]
fn node_db_rejects_invalid_hash_record() {
    init_stable_state();
    with_state_mut(|state| {
        let invalid = HashKey([9u8; 32]);
        apply_node_db_records(state, vec![(invalid, NodeRecord::new(1, vec![0x80]))]);
        assert_eq!(state.state_root_node_db.len(), 0);
    });
}

#[test]
fn account_leaf_hash_index_skips_dangling_hashes() {
    init_stable_state();
    with_state_mut(|state| {
        let addr = [0x11u8; 20];
        let rlp = vec![0x80];
        let valid_hash = HashKey(keccak256(&rlp));
        let dangling_hash = HashKey([0x77u8; 32]);
        apply_state_root_commit(
            state,
            PreparedStateRoot {
                state_root: [0u8; 32],
                storage_updates: Vec::new(),
                node_delta_counts: BTreeMap::from([(valid_hash, 1)]),
                new_node_records: BTreeMap::from([(valid_hash, rlp)]),
                updated_account_leaf_hashes: BTreeMap::from([
                    (make_account_key(addr), valid_hash),
                    (make_account_key([0x22u8; 20]), dangling_hash),
                ]),
                anchor_delta: AnchorDelta::default(),
            },
        );
        assert_eq!(
            state
                .state_root_account_leaf_hash
                .get(&make_account_key(addr))
                .map(|h| h.0),
            Some(valid_hash.0)
        );
        assert!(state
            .state_root_account_leaf_hash
            .get(&make_account_key([0x22u8; 20]))
            .is_none());
    });
}

#[test]
fn journal_includes_delta_only_addresses() {
    init_stable_state();
    with_state_mut(|state| {
        let addr = [0x33u8; 20];
        let slot = [0x01u8; 32];
        state
            .storage
            .insert(make_storage_key(addr, slot), U256Val::new([0x11u8; 32]));

        let mut delta = TrieDelta::default();
        let account = delta.accounts.entry(addr).or_default();
        account.storage.insert(slot, Some([0x22u8; 32]));

        let journal = super::trie_update::build_state_update_journal(state, &delta, &[]);
        assert_eq!(journal.storage_updates.len(), 1);
        assert_eq!(journal.storage_updates[0].addr, addr);
        assert!(journal.storage_updates[0].storage_root.is_some());
    });
}
