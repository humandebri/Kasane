//! どこで: produce_block 候補選択 / 何を: staged tx が current pending に由来する条件 / なぜ: replacement 後の旧 tx 実行を防ぐため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract staged_tx_is_current_pending_raw-8f305091 c98810eab47004dbbbc5b2e20198358cfbd1eee9186d381e8b7f7add0f4efb40
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (ready_points_to_tx == 1 && pending_meta_points_to_tx == 1 && current_pending_points_to_tx == 1 && tx_payload_present == 1 && tx_not_marked_dropped == 1),
))]
pub fn staged_tx_is_current_pending_raw(ready_points_to_tx: u64, pending_meta_points_to_tx: u64, current_pending_points_to_tx: u64, tx_payload_present: u64, tx_not_marked_dropped: u64) -> bool
{
    ready_points_to_tx == 1
        && pending_meta_points_to_tx == 1
        && current_pending_points_to_tx == 1
        && tx_payload_present == 1
        && tx_not_marked_dropped == 1
}
