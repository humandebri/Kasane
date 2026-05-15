//! どこで: EVM canister submit遷移 / 何を: nonce判定後のpending更新安全条件 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const SUBMIT_DECISION_ACCEPT: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const SUBMIT_DECISION_REPLACE: u64 = 1;

// specgen:contract submit_transition_safe_raw-3a7d7873 1b3bcc22b4af011df2aea133b3c9c57ba87f48ff492fa94ebf6df1cc3bfd5b40
pub fn submit_transition_safe_raw(
    decision_code: u64,
    pending_slot_points_to_new: u64,
    new_current_written: u64,
    queued_loc_written: u64,
    replacement_old_removed: u64,
) -> (result: bool)
    requires
        true,
    ensures
        result == ((decision_code == SUBMIT_DECISION_ACCEPT && pending_slot_points_to_new == 1 && new_current_written == 1 && queued_loc_written == 1 && replacement_old_removed == 0) || (decision_code == SUBMIT_DECISION_REPLACE && pending_slot_points_to_new == 1 && new_current_written == 1 && queued_loc_written == 1 && replacement_old_removed == 1)),
{
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
