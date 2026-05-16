//! どこで: canonical chain 更新境界 / 何を: append-only head と過去観測不変条件 / なぜ: reorg/rollback 不在を純粋条件化するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract no_reorg_append_only_raw-f8bac44c 6e5b41daf0f7dc892ed3726dbcde290dc8f3ec71994ce0eea9cbc5434046ce1c
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (previous_head < u64::MAX && committed_head == previous_head + 1 && parent_points_to_previous_head == 1 && previous_blocks_unchanged == 1 && previous_receipts_unchanged == 1 && previous_indexes_unchanged == 1),
))]
pub fn no_reorg_append_only_raw(previous_head: u64, committed_head: u64, parent_points_to_previous_head: u64, previous_blocks_unchanged: u64, previous_receipts_unchanged: u64, previous_indexes_unchanged: u64) -> bool
{
    previous_head < u64::MAX
        && committed_head == previous_head + 1
        && parent_points_to_previous_head == 1
        && previous_blocks_unchanged == 1
        && previous_receipts_unchanged == 1
        && previous_indexes_unchanged == 1
}
