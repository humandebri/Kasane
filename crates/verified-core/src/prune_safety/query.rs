//! どこで: pruning 後の query 観測 / 何を: pruned boundary と Ok/Pruned 応答の整合 / なぜ: 削除済み範囲と保持範囲の観測を矛盾させないため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract prune_query_observation_safe_raw-cb39bc8e c0cb83ae04627e437d3f197528eb0ccf29fbcef6d7aa84a5e0d9761383b2e15e
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (retained <= 1 && returned_ok <= 1 && returned_pruned <= 1 && ((block_number <= pruned_through && retained == 0 && returned_ok == 0 && returned_pruned == 1) || (pruned_through < block_number && returned_pruned == 0 && retained == returned_ok))),
))]
pub fn prune_query_observation_safe_raw(block_number: u64, pruned_through: u64, retained: u64, returned_ok: u64, returned_pruned: u64) -> bool
{
    retained <= 1
        && returned_ok <= 1
        && returned_pruned <= 1
        && ((block_number <= pruned_through
            && retained == 0
            && returned_ok == 0
            && returned_pruned == 1)
            || (pruned_through < block_number && returned_pruned == 0 && retained == returned_ok))
}
