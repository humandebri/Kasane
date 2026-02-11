# PR8 Signature Boundary Specification (Normative)

## Status (2026-02-11)

- This document remains normative for ingress/runtime signature boundary.
- Current API surface covered by this spec:
  - `submit_eth_tx`
  - `submit_ic_tx`
  - `rpc_eth_send_raw_transaction`
- Related management APIs (`set_mining_interval_ms`, `set_ops_config`) are removed from current public interface and are out of scope for PR8.

## 1. Scope

This document fixes the boundary between ingress-time transaction validation and EVM runtime precompile execution.

Target APIs:

- `submit_eth_tx`
- `submit_ic_tx`
- `rpc_eth_send_raw_transaction`

Reference implementation files:

- `crates/evm-core/src/tx_decode.rs`
- `crates/evm-core/src/chain.rs`
- `crates/evm-core/src/revm_exec.rs`
- `crates/ic-evm-wrapper/src/lib.rs`

## 2. Boundary Rules

### 2.1 Ingress Validation (MUST)

- EthSigned transaction validation MUST execute in this order:
  - EIP-2718 exact decode
  - chain_id validation
  - sender recovery by `recover_signer`
- IcSynthetic transaction validation MUST complete structural checks at ingress:
  - payload length
  - version
  - fee and nonce format constraints
- Submit/queue acceptance MUST only happen after `decode_tx` succeeds.
- Decode failure MUST be surfaced as ingress failure, not as runtime precompile failure.

### 2.2 Runtime Precompile (MUST)

- `ecrecover` and other precompile calls MUST be handled only inside EVM execution.
- Ingress sender validation MUST NOT depend on precompile execution.
- Runtime precompile failures MUST map to `exec.halt.precompile_error` in the execute error code family.

### 2.3 Duplicate Implementation (MUST NOT)

- The codebase MUST NOT introduce custom sender-signature verification that duplicates `recover_signer`.
- If future requirements need signature behavior changes, the implementation MUST be added via alloy/revm-compatible paths, not by ad-hoc duplicate ECDSA logic.

## 3. Public Error Code Mapping

Ingress submit APIs (`submit_eth_tx`, `submit_ic_tx`, `rpc_eth_send_raw_transaction`) MUST use stable machine-readable codes.
`ChainError` mapping MUST follow:

- `arg.tx_too_large`
- `arg.decode_failed`
- `arg.unsupported_tx_kind`
- `submit.tx_already_seen`
- `submit.invalid_fee`
- `submit.nonce_too_low`
- `submit.nonce_gap`
- `submit.nonce_conflict`
- `submit.queue_full`
- `submit.sender_queue_full`
- `internal.unexpected` (fallback for unexpected `ChainError`)

Ingress submit APIs MAY also return these pre-submit guard codes before `chain::submit_tx_in`:

- `auth.anonymous_forbidden`
- `ops.write.needs_migration`
- `ops.write.cycle_critical`
- `rpc.state_unavailable.corrupt_or_migrating` (`rpc_eth_send_raw_transaction` only)

Execute path mapping MUST keep `exec.*` for `ExecFailed(Some(err))` in wrapper execute mapping logic/tests.

## 4. Required Tests

- `chain_id` mismatch MUST be rejected before signature verification.
- Invalid signature MUST be rejected after passing chain_id checks.
- Runtime precompile failure MUST map to `exec.halt.precompile_error`.
- Ingress decode failure MUST map to `arg.decode_failed`.
- Unexpected chain error MUST map to `internal.unexpected`.

## 5. Update Procedure for New EIP Tx Types

When adding new EIP transaction support:

1. Extend decode logic in `tx_decode.rs` without changing the ingress boundary order.
2. Add/extend tests that prove chain_id and signature ordering still hold.
3. Update the stable error mapping table in this document.
4. Update reference-only notes in:
   - `docs/ops/fixplan2.md`
   - `README.md`
   - `docs/phase1.md`
5. Confirm `submit_*` and RPC submit APIs still return only stable codes.
