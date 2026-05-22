### replacement-strict-price

Yes. Replacement requires `incoming_effective_gas_price > pending_effective_gas_price`.
Equal effective gas price is rejected as `NonceDecision::Conflict`.

### nonce-order-precedence

Yes. `TooLow` and `Gap` take precedence over any pending replacement-price comparison.
Pending transaction data is only relevant when `incoming_nonce == expected_nonce`.
