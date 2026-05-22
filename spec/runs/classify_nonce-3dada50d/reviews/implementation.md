# implementation review: classify_nonce

The draft matches the control-flow order in `classify_nonce`: nonce ordering is checked before pending price comparison.

No invented business requirement was found in the inferred behavior. The intended behavior section remains explicitly unaccepted.

Review items for human confirmation:

- `TooLow` and `Gap` should take precedence over replacement price.
- Equal effective gas price should remain `Conflict`.
- `None` pending price at the expected nonce should mean `Accept`.
