# draft: prune_tx_cleanup_complete-171d1899

## inferred behavior
pub fn prune_tx_cleanup_complete(input: PruneTxCleanupInput) -> bool

## intended behavior
```rust
/// Returns true when all transaction cleanup artifacts are absent.
///
/// Candidate spec:
/// - Complete cleanup requires every tracked artifact to be removed.
/// - Returns true iff:
///   - tx_store is false
///   - receipt is false
///   - tx_index is false
///   - internal_traces is false
///   - tx_loc is false
///   - seen_tx is false
/// - Returns false if any artifact remains.
```

短縮版:

```text
prune_tx_cleanup_complete(input) == true
iff no transaction-related cleanup targets remain in tx_store, receipt,
tx_index, internal_traces, tx_loc, or seen_tx.
```

## anchor
- git_commit: a3bc9781ec94f42ff9edf5612aebd4f4532e69f0
- worktree_dirty: false
- source_hash: 171d18990ad8a287d272c1ba86712b88ac8bb578ba35829fba9190a1c081b67b
- semantic_hash: 40c4de421fdd22bb34024778e61718ac60846a9b1bb00c97570c0cee4ad93584
