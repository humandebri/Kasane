//! どこで: block production永続化前 / 何を: staged件数とcommit可否 / なぜ: 副作用前の不変条件を固定するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistBatchDecision {
    Commit,
    CountMismatch,
    Empty,
}

#[cfg_attr(verus_keep_ghost, verus_spec(decision => ensures
    included_count == 0 ==> decision == PersistBatchDecision::Empty,
    included_count != 0 && included_count != staged_count
        ==> decision == PersistBatchDecision::CountMismatch,
    included_count != 0 && included_count == staged_count
        ==> decision == PersistBatchDecision::Commit,
))]
pub fn classify_persist_batch(included_count: usize, staged_count: usize) -> PersistBatchDecision {
    if included_count == 0 {
        return PersistBatchDecision::Empty;
    }
    if included_count != staged_count {
        return PersistBatchDecision::CountMismatch;
    }
    PersistBatchDecision::Commit
}

#[cfg_attr(verus_keep_ghost, verus_spec(can_commit => ensures
    can_commit == (has_tx_index && has_receipt && block_number > 0),
))]
pub fn can_commit_single(has_tx_index: bool, has_receipt: bool, block_number: u64) -> bool {
    has_tx_index && has_receipt && block_number > 0
}

#[cfg(test)]
mod tests {
    use super::{can_commit_single, classify_persist_batch, PersistBatchDecision};

    #[test]
    fn persist_batch_requires_nonempty_matching_counts() {
        assert_eq!(classify_persist_batch(0, 0), PersistBatchDecision::Empty);
        assert_eq!(
            classify_persist_batch(2, 1),
            PersistBatchDecision::CountMismatch
        );
        assert_eq!(classify_persist_batch(2, 2), PersistBatchDecision::Commit);
    }

    #[test]
    fn single_commit_requires_index_receipt_and_nonzero_block() {
        assert!(can_commit_single(true, true, 1));
        assert!(!can_commit_single(false, true, 1));
        assert!(!can_commit_single(true, false, 1));
        assert!(!can_commit_single(true, true, 0));
    }
}
