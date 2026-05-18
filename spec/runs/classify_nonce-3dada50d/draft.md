# Spec draft: classify_nonce

## Function

`classify_nonce(expected_nonce: u64, incoming_nonce: u64, pending_effective_gas_price: Option<u64>, incoming_effective_gas_price: u64) -> NonceDecision`

## Source

- file: `crates/verified-core/src/nonce.rs`
- lines: 16-47
- git_commit: `703d7df1dcdc48c6f15be3733c6da4ec5c6a8dad`
- semantic_hash: `fa9487ba51d96176707f328ef5a7921718815d559e995dab958c1269eb12930b`

## Inferred behavior

- If `incoming_nonce < expected_nonce`, the function returns `NonceDecision::TooLow`.
- If `incoming_nonce > expected_nonce`, the function returns `NonceDecision::Gap`.
- If `incoming_nonce == expected_nonce` and no pending transaction price exists, the function returns `NonceDecision::Accept`.
- If `incoming_nonce == expected_nonce` and `incoming_effective_gas_price <= old_effective`, the function returns `NonceDecision::Conflict`.
- If `incoming_nonce == expected_nonce` and `incoming_effective_gas_price > old_effective`, the function returns `NonceDecision::Replace`.

## Intended behavior

Needs human review. The implementation appears to encode nonce ordering before replacement-fee comparison, but the business intent must be confirmed by scenario review.

## Preconditions

None inferred.

## Postconditions

- `incoming_nonce < expected_nonce ==> result == NonceDecision::TooLow`
- `incoming_nonce > expected_nonce ==> result == NonceDecision::Gap`
- `incoming_nonce == expected_nonce && pending_effective_gas_price == None ==> result == NonceDecision::Accept`
- `incoming_nonce == expected_nonce && pending_effective_gas_price == Some(old) && incoming_effective_gas_price <= old ==> result == NonceDecision::Conflict`
- `incoming_nonce == expected_nonce && pending_effective_gas_price == Some(old) && incoming_effective_gas_price > old ==> result == NonceDecision::Replace`

## Safety properties

- Nonce ordering decisions do not depend on pending transaction price.
- Replacement is allowed only when the incoming effective gas price is strictly greater than the pending effective gas price.
- Equal effective gas price is not sufficient for replacement.
- The function performs no arithmetic and should not overflow.
- The function has no inferred panic path.

## Ambiguities for review

- Confirm that equal fee replacement must be rejected as `Conflict`.
- Confirm that nonce `TooLow` and `Gap` take precedence over any pending transaction price.
- Confirm that absent pending transaction price at the expected nonce means `Accept`.
