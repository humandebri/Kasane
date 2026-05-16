Review as implementation, edge-case, adversarial, Verus:
pub fn prune_query_observation_safe_raw(
    block_number: u64,
    pruned_through: u64,
    retained: u64,
    returned_ok: u64,
    returned_pruned: u64,
) -> bool
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
