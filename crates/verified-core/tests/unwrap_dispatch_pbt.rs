//! どこで: unwrap dispatch PBT / 何を: retry・upgrade・terminal仕様 / なぜ: terminal再queueと二重dispatch仕様の誤りを検出するため

use proptest::prelude::*;
use verified_core::unwrap_dispatch::{
    unwrap_dispatch_terminal_raw, unwrap_dispatch_transition_safe_raw,
    unwrap_retry_transition_safe_raw, unwrap_upgrade_recovery_safe_raw, UNWRAP_STATUS_DISPATCHED,
    UNWRAP_STATUS_DISPATCHING, UNWRAP_STATUS_DISPATCH_FAILED, UNWRAP_STATUS_QUEUED,
};

fn expected_dispatch_transition(
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

fn expected_upgrade_recovery(
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

proptest! {
    #[test]
    fn pbt_unwrap_retry_and_terminal_are_exact(
        status in 0u64..6,
        next_status in 0u64..6,
        queue_inserted in 0u64..3,
        error_cleared in 0u64..3,
    ) {
        prop_assert_eq!(
            unwrap_dispatch_terminal_raw(status),
            status == UNWRAP_STATUS_DISPATCHED || status == UNWRAP_STATUS_DISPATCH_FAILED
        );
        prop_assert_eq!(
            unwrap_retry_transition_safe_raw(status, next_status, queue_inserted, error_cleared),
            status == UNWRAP_STATUS_DISPATCH_FAILED
                && next_status == UNWRAP_STATUS_QUEUED
                && queue_inserted == 1
                && error_cleared == 1
        );
    }

    #[test]
    fn pbt_unwrap_dispatch_transition_allows_only_canonical_edges(
        previous_status in 0u64..6,
        next_status in 0u64..6,
        ledger_tx_id_present in 0u64..3,
        error_present in 0u64..3,
        queue_inserted in 0u64..3,
    ) {
        prop_assert_eq!(
            unwrap_dispatch_transition_safe_raw(
                previous_status,
                next_status,
                ledger_tx_id_present,
                error_present,
                queue_inserted,
            ),
            expected_dispatch_transition(
                previous_status,
                next_status,
                ledger_tx_id_present,
                error_present,
                queue_inserted,
            )
        );
    }

    #[test]
    fn pbt_unwrap_upgrade_recovery_requeues_only_live_work(
        previous_status in 0u64..6,
        next_status in 0u64..6,
        queue_already_had_id in 0u64..3,
        queue_inserted in 0u64..3,
        timestamp_updated in 0u64..3,
    ) {
        prop_assert_eq!(
            unwrap_upgrade_recovery_safe_raw(
                previous_status,
                next_status,
                queue_already_had_id,
                queue_inserted,
                timestamp_updated,
            ),
            expected_upgrade_recovery(
                previous_status,
                next_status,
                queue_already_had_id,
                queue_inserted,
                timestamp_updated,
            )
        );
    }
}
