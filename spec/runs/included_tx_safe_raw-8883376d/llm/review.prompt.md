Review as implementation, edge-case, adversarial, Verus:
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
) -> bool
{
    has_tx_index
        && has_receipt
        && block_number > 0
        && receipt_tx_id_matches
        && index_key_matches_tx_id
        && loc_matches_position
        && receipt_matches_position
        && index_matches_position
}
