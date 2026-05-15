Generate a concise spec draft candidate:
#[cfg_attr(verus_keep_ghost, verus_spec(safe => ensures
    safe == (
        facts.has_tx_index
        && facts.has_receipt
        && facts.receipt_tx_id_matches
        && facts.index_key_matches_tx_id
        && facts.loc_matches_position
        && facts.receipt_matches_position
        && facts.index_matches_position
        && facts.block_number > 0
    ),
))]
pub fn included_tx_safe(facts: IncludedTxFacts) -> bool
{
    facts.has_tx_index
        && facts.has_receipt
        && facts.block_number > 0
        && facts.receipt_tx_id_matches
        && facts.index_key_matches_tx_id
        && facts.loc_matches_position
        && facts.receipt_matches_position
        && facts.index_matches_position
}
