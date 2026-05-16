//! どこで: upgrade 境界モデル / 何を: 永続化済み観測の保持条件 / なぜ: IC runtime を境界に置きつつ codec/map結線を証拠化するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract upgrade_core_observation_preserved_raw-191130e4 e00562e4e451eadbc30e2148ad744959cffb2c4f4e96eb1a9b0b8779f7bbd70c
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (head_same == 1 && pruned_boundary_same == 1 && pending_same == 1 && receipt_same == 1 && tx_index_same == 1 && tx_loc_same == 1),
))]
pub fn upgrade_core_observation_preserved_raw(head_same: u64, pruned_boundary_same: u64, pending_same: u64, receipt_same: u64, tx_index_same: u64, tx_loc_same: u64) -> bool
{
    head_same == 1
        && pruned_boundary_same == 1
        && pending_same == 1
        && receipt_same == 1
        && tx_index_same == 1
        && tx_loc_same == 1
}
