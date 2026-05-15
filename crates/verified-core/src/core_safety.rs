//! どこで: EVM canister中核遷移 / 何を: submit・include・commitの安全条件 / なぜ: 外部境界を除いた不変条件を固定するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const SUBMIT_DECISION_ACCEPT: u8 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const SUBMIT_DECISION_REPLACE: u8 = 1;

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
) -> bool {
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

#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        has_tx_index
        && has_receipt
        && receipt_tx_id_matches
        && index_key_matches_tx_id
        && loc_matches_position
        && receipt_matches_position
        && index_matches_position
        && block_number > 0
    ),
))]
pub fn included_tx_safe_raw(
    has_tx_index: bool,
    has_receipt: bool,
    receipt_tx_id_matches: bool,
    index_key_matches_tx_id: bool,
    loc_matches_position: bool,
    receipt_matches_position: bool,
    index_matches_position: bool,
    block_number: u64,
) -> bool {
    has_tx_index
        && has_receipt
        && block_number > 0
        && receipt_tx_id_matches
        && index_key_matches_tx_id
        && loc_matches_position
        && receipt_matches_position
        && index_matches_position
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
    included_count: usize,
    staged_count: usize,
    safe_included_count: usize,
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
