//! どこで: batch走査境界 / 何を: cursor/done/count遷移 / なぜ: stable map走査の再開条件を検証可能にするため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_spec(next => ensures
    processed < u64::MAX ==> next == processed + 1,
    processed == u64::MAX ==> next == u64::MAX,
    next >= processed,
))]
pub fn increment_processed(processed: u64) -> u64 {
    processed.saturating_add(1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(done => ensures
    done == (iterator_exhausted || processed < limit as u64),
))]
pub fn batch_done(processed: u64, limit: u32, iterator_exhausted: bool) -> bool {
    iterator_exhausted || processed < u64::from(limit)
}

#[cfg_attr(verus_keep_ghost, verus_spec(next => ensures
    last_seq < u64::MAX ==> next == last_seq + 1,
    last_seq == u64::MAX ==> next == u64::MAX,
    next >= last_seq,
))]
pub fn next_exclusive_cursor(last_seq: u64) -> u64 {
    last_seq.saturating_add(1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(reached => ensures
    reached == (item_count >= limit),
))]
pub fn snapshot_limit_reached(item_count: usize, limit: usize) -> bool {
    item_count >= limit
}

#[cfg(test)]
mod tests {
    use super::{batch_done, increment_processed, next_exclusive_cursor, snapshot_limit_reached};

    #[test]
    fn processed_count_saturates() {
        assert_eq!(increment_processed(0), 1);
        assert_eq!(increment_processed(u64::MAX), u64::MAX);
    }

    #[test]
    fn batch_done_matches_iterator_and_limit_state() {
        assert!(batch_done(0, 10, true));
        assert!(batch_done(9, 10, false));
        assert!(!batch_done(10, 10, false));
        assert!(!batch_done(0, 0, false));
    }

    #[test]
    fn cursors_and_snapshot_capacity_are_monotonic() {
        assert_eq!(next_exclusive_cursor(7), 8);
        assert_eq!(next_exclusive_cursor(u64::MAX), u64::MAX);
        assert!(snapshot_limit_reached(2, 2));
        assert!(!snapshot_limit_reached(1, 2));
    }
}
