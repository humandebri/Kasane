Generate a concise spec draft candidate:
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
