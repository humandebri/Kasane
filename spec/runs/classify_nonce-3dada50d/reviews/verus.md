# Verus review: classify_nonce

The postconditions are directly expressible as branch-complete `ensures` clauses.

No precondition is required because all inputs are total over `u64` and `Option<u64>`.

The existing `#[cfg_attr(verus_keep_ghost, verus_spec(...))]` annotation already encodes the same branch cases.
