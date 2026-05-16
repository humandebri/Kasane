//! どこで: verified-core P0 safety / 何を: append-only・staging・bidirectional index の public API / なぜ: adapter evidence の純粋境界を固定するため

use verified_core::no_reorg::no_reorg_append_only_raw;
use verified_core::receipt_index::{receipt_index_location_bidirectional, ReceiptIndexObservation};
use verified_core::staging::staged_tx_is_current_pending_raw;

#[test]
fn no_reorg_append_only_requires_next_head_and_old_observations_unchanged() {
    assert!(no_reorg_append_only_raw(7, 8, 1, 1, 1, 1));
    assert!(!no_reorg_append_only_raw(7, 7, 1, 1, 1, 1));
    assert!(!no_reorg_append_only_raw(7, 9, 1, 1, 1, 1));
    assert!(!no_reorg_append_only_raw(7, 8, 0, 1, 1, 1));
    assert!(!no_reorg_append_only_raw(7, 8, 1, 0, 1, 1));
    assert!(!no_reorg_append_only_raw(u64::MAX, 0, 1, 1, 1, 1));
}

#[test]
fn staged_tx_requires_current_pending_and_live_payload() {
    assert!(staged_tx_is_current_pending_raw(1, 1, 1, 1, 1));
    assert!(!staged_tx_is_current_pending_raw(0, 1, 1, 1, 1));
    assert!(!staged_tx_is_current_pending_raw(1, 0, 1, 1, 1));
    assert!(!staged_tx_is_current_pending_raw(1, 1, 0, 1, 1));
    assert!(!staged_tx_is_current_pending_raw(1, 1, 1, 0, 1));
    assert!(!staged_tx_is_current_pending_raw(1, 1, 1, 1, 0));
}

#[test]
fn receipt_index_location_bidirectional_requires_all_reverse_links() {
    let ok = ReceiptIndexObservation {
        tx_index_present: true,
        receipt_present: true,
        included_loc_present: true,
        index_matches_loc: true,
        receipt_matches_loc: true,
        loc_points_to_block_tx: true,
    };
    assert!(receipt_index_location_bidirectional(ok));

    assert!(!receipt_index_location_bidirectional(
        ReceiptIndexObservation {
            receipt_present: false,
            ..ok
        }
    ));
    assert!(!receipt_index_location_bidirectional(
        ReceiptIndexObservation {
            tx_index_present: false,
            ..ok
        }
    ));
    assert!(!receipt_index_location_bidirectional(
        ReceiptIndexObservation {
            included_loc_present: false,
            ..ok
        }
    ));
    assert!(!receipt_index_location_bidirectional(
        ReceiptIndexObservation {
            loc_points_to_block_tx: false,
            ..ok
        }
    ));
}
