Generate a concise spec draft candidate:
pub fn prune_partial_progress_safe_raw(
    previous_present: u64,
    previous_boundary: u64,
    next_present: u64,
    next_boundary: u64,
    next_cursor: u64,
    max_ops: u64,
    ops_used: u64,
    next_ops_needed: u64,
    did_work: u64,
    stopped_for_budget: u64,
) -> bool
{
    previous_present <= 1
        && next_present <= 1
        && did_work <= 1
        && stopped_for_budget <= 1
        && ops_used <= max_ops
        && (did_work == 0 || next_present == 1)
        && (previous_present == 0 || next_present == 1)
        && (did_work == 0 || previous_present == 0 || previous_boundary < next_boundary)
        && (previous_present == 0
            || next_present == 0
            || previous_boundary == next_boundary
            || did_work == 1)
        && (previous_present == 0 || next_present == 0 || previous_boundary <= next_boundary)
        && (next_present == 0 || next_boundary < next_cursor)
        && (stopped_for_budget == 0 || next_present == 1)
        && (stopped_for_budget == 0
            || max_ops < next_ops_needed
            || (next_ops_needed <= max_ops && max_ops - next_ops_needed < ops_used))
}
