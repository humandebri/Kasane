//! どこで: state root永続化層 / 何を: journalをstableに適用 / なぜ: 副作用境界を一本化するため

use alloy_trie::EMPTY_ROOT_HASH;
use evm_db::chain_data::{mark_decode_failure, HashKey, NodeRecord};
use evm_db::stable_state::StableState;
use std::collections::BTreeMap;

pub type NodeDeltaCounts = BTreeMap<HashKey, i64>;
pub type NewNodeRecords = BTreeMap<HashKey, Vec<u8>>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AnchorDelta {
    pub state_root_old: Option<HashKey>,
    pub state_root_new: Option<HashKey>,
    pub storage_root_old: Vec<HashKey>,
    pub storage_root_new: Vec<HashKey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalUpdate {
    pub node_delta_counts: NodeDeltaCounts,
    pub new_node_records: NewNodeRecords,
    pub anchor_delta: AnchorDelta,
}

pub fn apply_journal(state: &mut StableState, mut journal: JournalUpdate) {
    merge_anchor_delta(&mut journal.node_delta_counts, &journal.anchor_delta);

    for (hash, rlp) in journal.new_node_records {
        if let Some(existing) = state.state_root_node_db.get(&hash) {
            if existing.rlp == rlp {
                continue;
            }
        }
        state
            .state_root_node_db
            .insert(hash, NodeRecord::new(0, rlp));
    }

    for (hash, delta) in journal.node_delta_counts {
        if delta == 0 {
            continue;
        }
        let existing = state.state_root_node_db.get(&hash);
        if existing.is_none() && delta > 0 {
            let mut empty_root = [0u8; 32];
            empty_root.copy_from_slice(EMPTY_ROOT_HASH.as_slice());
            if hash.0 == empty_root {
                continue;
            }
            mark_decode_failure(b"state_root_journal_inconsistent", true);
            continue;
        }
        let Some(mut record) = existing else { continue };
        let next = (i128::from(record.refcnt) + i128::from(delta)).max(0) as u32;
        if record.refcnt > 0 && next == 0 {
            enqueue_gc_hash(state, hash);
        }
        record.refcnt = next;
        state.state_root_node_db.insert(hash, record);
    }

    run_gc_tick(state, 256);
    refresh_metrics(state);
}

fn merge_anchor_delta(counts: &mut NodeDeltaCounts, anchors: &AnchorDelta) {
    if let Some(hash) = anchors.state_root_old {
        add_delta(counts, hash, -1);
    }
    if let Some(hash) = anchors.state_root_new {
        add_delta(counts, hash, 1);
    }
    for hash in anchors.storage_root_old.iter().copied() {
        add_delta(counts, hash, -1);
    }
    for hash in anchors.storage_root_new.iter().copied() {
        add_delta(counts, hash, 1);
    }
}

fn add_delta(counts: &mut NodeDeltaCounts, hash: HashKey, delta: i64) {
    let entry = counts.entry(hash).or_insert(0);
    *entry += delta;
    if *entry == 0 {
        counts.remove(&hash);
    }
}

fn enqueue_gc_hash(state: &mut StableState, hash: HashKey) {
    let mut gc = *state.state_root_gc_state.get();
    let seq = gc.enqueue_seq;
    state.state_root_gc_queue.insert(seq, hash);
    gc.enqueue_seq = gc.enqueue_seq.saturating_add(1);
    gc.len = gc.len.saturating_add(1);
    state.state_root_gc_state.set(gc);
}

fn run_gc_tick(state: &mut StableState, max_steps: u64) {
    let mut gc = *state.state_root_gc_state.get();
    let mut steps = 0u64;
    while steps < max_steps && gc.len > 0 {
        let seq = gc.dequeue_seq;
        if let Some(hash) = state.state_root_gc_queue.remove(&seq) {
            if let Some(record) = state.state_root_node_db.get(&hash) {
                if record.refcnt == 0 {
                    state.state_root_node_db.remove(&hash);
                }
            }
        }
        gc.dequeue_seq = gc.dequeue_seq.saturating_add(1);
        gc.len = gc.len.saturating_sub(1);
        steps = steps.saturating_add(1);
    }
    state.state_root_gc_state.set(gc);
}

fn refresh_metrics(state: &mut StableState) {
    let mut metrics = *state.state_root_metrics.get();
    let mut reachable = 0u64;
    let mut unreachable = 0u64;
    for entry in state.state_root_node_db.iter() {
        if entry.value().refcnt > 0 {
            reachable = reachable.saturating_add(1);
        } else {
            unreachable = unreachable.saturating_add(1);
        }
    }
    metrics.node_db_entries = state.state_root_node_db.len();
    metrics.node_db_reachable = reachable;
    metrics.node_db_unreachable = unreachable;
    metrics.gc_progress = state.state_root_gc_state.get().dequeue_seq;
    state.state_root_metrics.set(metrics);
}

#[cfg(test)]
mod tests {
    use super::{apply_journal, AnchorDelta, JournalUpdate, NewNodeRecords, NodeDeltaCounts};
    use evm_db::chain_data::HashKey;
    use evm_db::meta::{clear_needs_migration, needs_migration};
    use evm_db::stable_state::{init_stable_state, with_state_mut};
    use std::collections::BTreeMap;

    #[test]
    fn inconsistent_journal_marks_decode_failure_instead_of_panicking() {
        init_stable_state();
        clear_needs_migration();

        with_state_mut(|state| {
            let mut deltas: NodeDeltaCounts = BTreeMap::new();
            let inconsistent = HashKey([0x11u8; 32]);
            deltas.insert(inconsistent, 1);
            apply_journal(
                state,
                JournalUpdate {
                    node_delta_counts: deltas,
                    new_node_records: NewNodeRecords::new(),
                    anchor_delta: AnchorDelta::default(),
                },
            );
            assert!(state.state_root_node_db.get(&inconsistent).is_none());
        });

        assert!(needs_migration());
    }
}
