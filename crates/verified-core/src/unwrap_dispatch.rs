//! どこで: unwrap dispatch 境界 / 何を: retry・upgrade復旧・terminal状態 / なぜ: 二重dispatchとterminal再queueを防ぐため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const UNWRAP_STATUS_QUEUED: u64 = 0;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const UNWRAP_STATUS_DISPATCHING: u64 = 1;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const UNWRAP_STATUS_DISPATCHED: u64 = 2;
#[cfg_attr(verus_keep_ghost, verus_verify)]
pub const UNWRAP_STATUS_DISPATCH_FAILED: u64 = 3;

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        status == UNWRAP_STATUS_DISPATCHED
        || status == UNWRAP_STATUS_DISPATCH_FAILED
    ),
))]
pub fn unwrap_dispatch_terminal_raw(status: u64) -> bool {
    status == UNWRAP_STATUS_DISPATCHED || status == UNWRAP_STATUS_DISPATCH_FAILED
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        previous_status == UNWRAP_STATUS_DISPATCH_FAILED
        && next_status == UNWRAP_STATUS_QUEUED
        && queue_inserted == 1
        && error_cleared == 1
    ),
))]
pub fn unwrap_retry_transition_safe_raw(
    previous_status: u64,
    next_status: u64,
    queue_inserted: u64,
    error_cleared: u64,
) -> bool {
    previous_status == UNWRAP_STATUS_DISPATCH_FAILED
        && next_status == UNWRAP_STATUS_QUEUED
        && queue_inserted == 1
        && error_cleared == 1
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        (previous_status == UNWRAP_STATUS_QUEUED
            && next_status == UNWRAP_STATUS_DISPATCHING
            && ledger_tx_id_present == 0
            && error_present == 0
            && queue_inserted == 0)
        || (previous_status == UNWRAP_STATUS_DISPATCHING
            && next_status == UNWRAP_STATUS_DISPATCHED
            && ledger_tx_id_present == 1
            && error_present == 0
            && queue_inserted == 0)
        || (previous_status == UNWRAP_STATUS_DISPATCHING
            && next_status == UNWRAP_STATUS_DISPATCH_FAILED
            && ledger_tx_id_present == 0
            && error_present == 1
            && queue_inserted == 0)
        || (previous_status == UNWRAP_STATUS_DISPATCH_FAILED
            && next_status == UNWRAP_STATUS_QUEUED
            && ledger_tx_id_present == 0
            && error_present == 0
            && queue_inserted == 1)
    ),
))]
pub fn unwrap_dispatch_transition_safe_raw(
    previous_status: u64,
    next_status: u64,
    ledger_tx_id_present: u64,
    error_present: u64,
    queue_inserted: u64,
) -> bool {
    (previous_status == UNWRAP_STATUS_QUEUED
        && next_status == UNWRAP_STATUS_DISPATCHING
        && ledger_tx_id_present == 0
        && error_present == 0
        && queue_inserted == 0)
        || (previous_status == UNWRAP_STATUS_DISPATCHING
            && next_status == UNWRAP_STATUS_DISPATCHED
            && ledger_tx_id_present == 1
            && error_present == 0
            && queue_inserted == 0)
        || (previous_status == UNWRAP_STATUS_DISPATCHING
            && next_status == UNWRAP_STATUS_DISPATCH_FAILED
            && ledger_tx_id_present == 0
            && error_present == 1
            && queue_inserted == 0)
        || (previous_status == UNWRAP_STATUS_DISPATCH_FAILED
            && next_status == UNWRAP_STATUS_QUEUED
            && ledger_tx_id_present == 0
            && error_present == 0
            && queue_inserted == 1)
}

#[cfg_attr(verus_keep_ghost, verus_spec(result => ensures
    result == (
        (previous_status == UNWRAP_STATUS_QUEUED
            && next_status == UNWRAP_STATUS_QUEUED
            && timestamp_updated == 0
            && ((queue_already_had_id == 1 && queue_inserted == 0)
                || (queue_already_had_id == 0 && queue_inserted == 1)))
        || (previous_status == UNWRAP_STATUS_DISPATCHING
            && next_status == UNWRAP_STATUS_QUEUED
            && queue_already_had_id == 0
            && queue_inserted == 1
            && timestamp_updated == 1)
        || ((previous_status == UNWRAP_STATUS_DISPATCHED
            || previous_status == UNWRAP_STATUS_DISPATCH_FAILED)
            && next_status == previous_status
            && queue_inserted == 0
            && timestamp_updated == 0)
    ),
))]
pub fn unwrap_upgrade_recovery_safe_raw(
    previous_status: u64,
    next_status: u64,
    queue_already_had_id: u64,
    queue_inserted: u64,
    timestamp_updated: u64,
) -> bool {
    (previous_status == UNWRAP_STATUS_QUEUED
        && next_status == UNWRAP_STATUS_QUEUED
        && timestamp_updated == 0
        && ((queue_already_had_id == 1 && queue_inserted == 0)
            || (queue_already_had_id == 0 && queue_inserted == 1)))
        || (previous_status == UNWRAP_STATUS_DISPATCHING
            && next_status == UNWRAP_STATUS_QUEUED
            && queue_already_had_id == 0
            && queue_inserted == 1
            && timestamp_updated == 1)
        || ((previous_status == UNWRAP_STATUS_DISPATCHED
            || previous_status == UNWRAP_STATUS_DISPATCH_FAILED)
            && next_status == previous_status
            && queue_inserted == 0
            && timestamp_updated == 0)
}
