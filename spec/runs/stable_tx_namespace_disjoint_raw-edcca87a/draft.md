# draft: stable_tx_namespace_disjoint_raw-edcca87a

## inferred behavior
pub fn stable_tx_namespace_disjoint_raw(seen_tx: u64, tx_store: u64, tx_index: u64, receipts: u64, tx_locs: u64, tx_locs_v3: u64, internal_traces: u64) -> bool

## intended behavior
仕様候補:

```text
Purpose:
stable memory namespace IDs for transaction-related structures are strictly ordered,
therefore pairwise disjoint by construction.

Preconditions:
true

Postcondition:
result ==
    seen_tx < tx_store
    && tx_store < tx_index
    && tx_index < receipts
    && receipts < tx_locs
    && tx_locs < tx_locs_v3
    && tx_locs_v3 < internal_traces

Acceptance criterion:
Returns true iff all seven namespace IDs form the required strictly increasing order.
Returns false when any adjacent ordering constraint is equal or reversed.
```

`specgen spec add-post` 用:

```text
result == (seen_tx < tx_store
    && tx_store < tx_index
    && tx_index < receipts
    && receipts < tx_locs
    && tx_locs < tx_locs_v3
    && tx_locs_v3 < internal_traces)
```

## anchor
- git_commit: 655eb69df86c6f5467bc646f165eda8ed95d7fb9
- worktree_dirty: true
- source_hash: edcca87a73f1fc2393350930c833bf1f61ca2674e1dc15a3abd75213aa841567
- semantic_hash: fa398a53c3b9462e36a27f99a9162cdadb5bf55083bd94388b596ab41735666c
