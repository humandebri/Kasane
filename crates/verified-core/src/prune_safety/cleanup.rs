//! どこで: pruning削除後観測 / 何を: tx関連索引が観測不能か / なぜ: receipt/index/query整合性のadapter前提にするため

#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

#[cfg_attr(verus_keep_ghost, verus_verify)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PruneTxCleanupInput {
    pub tx_store: bool,
    pub receipt: bool,
    pub tx_index: bool,
    pub internal_traces: bool,
    pub tx_loc: bool,
    pub seen_tx: bool,
}

// specgen:contract prune_tx_cleanup_complete-171d1899 eaa6fc361e08aa330d07782b3acb9f2ae27626ffe0c5cd251c7ed4bab3162321
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    requires
        true,
    ensures
        result == (!input.tx_store && !input.receipt && !input.tx_index && !input.internal_traces && !input.tx_loc && !input.seen_tx),
))]
pub fn prune_tx_cleanup_complete(input: PruneTxCleanupInput) -> bool
{
    !input.tx_store
        && !input.receipt
        && !input.tx_index
        && !input.internal_traces
        && !input.tx_loc
        && !input.seen_tx
}
