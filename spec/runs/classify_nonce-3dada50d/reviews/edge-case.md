# edge-case review: classify_nonce

The function performs only comparison and pattern matching, so no arithmetic overflow is inferred.

Covered edge cases:

- incoming nonce below expected nonce
- incoming nonce above expected nonce
- matching nonce with no pending price
- matching nonce with equal pending price
- matching nonce with lower incoming price
- matching nonce with higher incoming price

No panic path is inferred from the target function body.
