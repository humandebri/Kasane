Generate a concise spec draft candidate:
pub fn stable_tx_namespace_disjoint_raw(seen_tx: u64, tx_store: u64, tx_index: u64, receipts: u64, tx_locs: u64, tx_locs_v3: u64, internal_traces: u64) -> bool
{
    seen_tx < tx_store
        && tx_store < tx_index
        && tx_index < receipts
        && receipts < tx_locs
        && tx_locs < tx_locs_v3
        && tx_locs_v3 < internal_traces
}
