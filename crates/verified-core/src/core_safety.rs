//! どこで: EVM canister中核遷移 / 何を: submit・include・commitの安全条件 / なぜ: 外部境界を除いた不変条件を固定するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const SUBMIT_DECISION_ACCEPT: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const SUBMIT_DECISION_REPLACE: u64 = 1;

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        (decision_code == SUBMIT_DECISION_ACCEPT
            && pending_slot_points_to_new == 1
            && new_current_written == 1
            && queued_loc_written == 1
            && replacement_old_removed == 0)
        || (decision_code == SUBMIT_DECISION_REPLACE
            && pending_slot_points_to_new == 1
            && new_current_written == 1
            && queued_loc_written == 1
            && replacement_old_removed == 1)
    ),
))]
pub fn submit_transition_safe_raw(
    decision_code: u64,
    pending_slot_points_to_new: u64,
    new_current_written: u64,
    queued_loc_written: u64,
    replacement_old_removed: u64,
) -> bool {
    (decision_code == SUBMIT_DECISION_ACCEPT
        && pending_slot_points_to_new == 1
        && new_current_written == 1
        && queued_loc_written == 1
        && replacement_old_removed == 0)
        || (decision_code == SUBMIT_DECISION_REPLACE
            && pending_slot_points_to_new == 1
            && new_current_written == 1
            && queued_loc_written == 1
            && replacement_old_removed == 1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        has_tx_index == 1
        && has_receipt == 1
        && receipt_tx_id_matches == 1
        && index_key_matches_tx_id == 1
        && loc_matches_position == 1
        && receipt_matches_position == 1
        && index_matches_position == 1
        && block_number > 0
    ),
))]
pub fn included_tx_safe_raw(
    has_tx_index: u64,
    has_receipt: u64,
    receipt_tx_id_matches: u64,
    index_key_matches_tx_id: u64,
    loc_matches_position: u64,
    receipt_matches_position: u64,
    index_matches_position: u64,
    block_number: u64,
) -> bool {
    has_tx_index == 1
        && has_receipt == 1
        && block_number > 0
        && receipt_tx_id_matches == 1
        && index_key_matches_tx_id == 1
        && loc_matches_position == 1
        && receipt_matches_position == 1
        && index_matches_position == 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        previous_head < u64::MAX
        && committed_head == previous_head + 1
        && (block_gas_limit == 0 || block_gas_used <= block_gas_limit)
        && included_count != 0
        && included_count == staged_count
        && safe_included_count == included_count
    ),
))]
pub fn block_commit_safe_raw(
    previous_head: u64,
    committed_head: u64,
    included_count: u64,
    staged_count: u64,
    safe_included_count: u64,
    block_gas_used: u64,
    block_gas_limit: u64,
) -> bool {
    previous_head < u64::MAX
        && committed_head == previous_head + 1
        && (block_gas_limit == 0 || block_gas_used <= block_gas_limit)
        && included_count != 0
        && included_count == staged_count
        && safe_included_count == included_count
}
