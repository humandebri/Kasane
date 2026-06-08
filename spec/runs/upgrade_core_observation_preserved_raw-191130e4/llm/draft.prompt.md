Generate a concise spec draft candidate:
pub fn upgrade_core_observation_preserved_raw(head_same: u64, pruned_boundary_same: u64, pending_same: u64, receipt_same: u64, tx_index_same: u64, tx_loc_same: u64) -> bool
{
    head_same == 1
        && pruned_boundary_same == 1
        && pending_same == 1
        && receipt_same == 1
        && tx_index_same == 1
        && tx_loc_same == 1
}
