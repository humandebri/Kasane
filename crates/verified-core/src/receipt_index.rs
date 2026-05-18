//! どこで: included tx の観測境界 / 何を: receipt/index/location の双方向整合 / なぜ: 片方向の永続化主張を query 観測へ接続するため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReceiptIndexObservation {
    pub tx_index_present: bool,
    pub receipt_present: bool,
    pub included_loc_present: bool,
    pub index_matches_loc: bool,
    pub receipt_matches_loc: bool,
    pub loc_points_to_block_tx: bool,
}

// specgen:contract receipt_index_location_bidirectional-7f362e2c 075accd33b10181d1a2abecfa98ebbde6583902282a1ac2bd3e3e8fcf9fb9679
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == ((!input.tx_index_present || (input.receipt_present && input.included_loc_present && input.index_matches_loc)) && (!input.receipt_present || (input.tx_index_present && input.included_loc_present && input.receipt_matches_loc)) && (!input.included_loc_present || (input.tx_index_present && input.receipt_present && input.index_matches_loc && input.receipt_matches_loc && input.loc_points_to_block_tx))),
))]
#[allow(clippy::nonminimal_bool)]
pub fn receipt_index_location_bidirectional(input: ReceiptIndexObservation) -> bool
{
    (!input.tx_index_present
        || (input.receipt_present && input.included_loc_present && input.index_matches_loc))
        && (!input.receipt_present
            || (input.tx_index_present && input.included_loc_present && input.receipt_matches_loc))
        && (!input.included_loc_present
            || (input.tx_index_present
                && input.receipt_present
                && input.index_matches_loc
                && input.receipt_matches_loc
                && input.loc_points_to_block_tx))
}
