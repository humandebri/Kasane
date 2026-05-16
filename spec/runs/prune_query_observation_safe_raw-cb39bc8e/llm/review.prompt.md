Review as implementation, edge-case, adversarial, Verus:
pub fn prune_query_observation_safe_raw(
    boundary_present: u64,
    block_number: u64,
    pruned_before: u64,
    retained: u64,
    returned_ok: u64,
    returned_pruned: u64,
) -> bool
{
    boundary_present <= 1
        && retained <= 1
        && returned_ok <= 1
        && returned_pruned <= 1
        && (returned_ok == 0 || retained != 0)
        && (boundary_present == 0 || block_number > pruned_before || returned_ok == 0)
        && (retained == 0 || returned_pruned == 0)
        && !(returned_ok != 0 && returned_pruned != 0)
        && (returned_pruned == 0 || (boundary_present != 0 && block_number <= pruned_before))
}
