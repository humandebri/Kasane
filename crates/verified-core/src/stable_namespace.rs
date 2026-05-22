//! どこで: stable memory namespace / 何を: tx関連mapのMemoryId分離 / なぜ: 同一key型のmap衝突をadapter境界で防ぐため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

// specgen:contract stable_tx_namespace_disjoint_raw-edcca87a a2cac3332aac62f5ed0418cb3b3c4dbe54a8df812f021f900e764db13d0dc8a6
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (seen_tx < tx_store && tx_store < tx_index && tx_index < receipts && receipts < tx_locs && tx_locs < tx_locs_v3 && tx_locs_v3 < internal_traces),
))]
pub fn stable_tx_namespace_disjoint_raw(seen_tx: u64, tx_store: u64, tx_index: u64, receipts: u64, tx_locs: u64, tx_locs_v3: u64, internal_traces: u64) -> bool
{
    seen_tx < tx_store
        && tx_store < tx_index
        && tx_index < receipts
        && receipts < tx_locs
        && tx_locs < tx_locs_v3
        && tx_locs_v3 < internal_traces
}
