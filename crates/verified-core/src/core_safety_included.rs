//! どこで: EVM canister include遷移 / 何を: receipt/index/location整合性 / なぜ: specgen contractを単一targetファイルへ注入するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract included_tx_safe_raw-8883376d bb83f6a6256a26696727c069a8fdbfcf70d178a85c4e4bac1bfa5dfc36fc9311
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (has_tx_index == 1 && has_receipt == 1 && receipt_tx_id_matches == 1 && index_key_matches_tx_id == 1 && loc_matches_position == 1 && receipt_matches_position == 1 && index_matches_position == 1 && block_number > 0),
))]
pub fn included_tx_safe_raw(has_tx_index: u64, has_receipt: u64, receipt_tx_id_matches: u64, index_key_matches_tx_id: u64, loc_matches_position: u64, receipt_matches_position: u64, index_matches_position: u64, block_number: u64) -> bool
{
    has_tx_index == 1
        && has_receipt == 1
        && block_number > 0
        && receipt_tx_id_matches == 1
        && index_key_matches_tx_id == 1
        && loc_matches_position == 1
        && receipt_matches_position == 1
        && index_matches_position == 1
}
