Generate scenario candidates:
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        (decision_code == SUBMIT_DECISION_ACCEPT
            && pending_slot_points_to_new
            && new_current_written
            && queued_loc_written
            && !replacement_old_removed)
        || (decision_code == SUBMIT_DECISION_REPLACE
            && pending_slot_points_to_new
            && new_current_written
            && queued_loc_written
            && replacement_old_removed)
    ),
))]
pub fn submit_transition_safe_raw(
    decision_code: u8,
    pending_slot_points_to_new: bool,
    new_current_written: bool,
    queued_loc_written: bool,
    replacement_old_removed: bool,
) -> bool
{
    (decision_code == SUBMIT_DECISION_ACCEPT
        && pending_slot_points_to_new
        && new_current_written
        && queued_loc_written
        && !replacement_old_removed)
        || (decision_code == SUBMIT_DECISION_REPLACE
            && pending_slot_points_to_new
            && new_current_written
            && queued_loc_written
            && replacement_old_removed)
}
