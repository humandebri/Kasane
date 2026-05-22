Generate a concise spec draft candidate:
pub fn prune_tx_cleanup_complete(input: PruneTxCleanupInput) -> bool
{
    !input.tx_store
        && !input.receipt
        && !input.tx_index
        && !input.internal_traces
        && !input.tx_loc
        && !input.seen_tx
}
