# Verification Architecture

Verus-targeted code lives under `crates/verified-*`. Canister implementation code is limited to adapters for the IC runtime, stable memory, Candid, time, cycles, `revm`, hashing, and codecs.

## Boundaries

- `crates/verified-core`: pure state transitions for fees, nonce, queueing, blocks, batches, transaction indexes, pruning, stable codecs, and state diffs. Adapter code calls the same implementation functions directly.
- `crates/evm-core`: stable-state reads/writes, `revm` execution, Candid/API input, and metrics updates.
- `crates/evm-db`: stable-memory byte codecs and map key/value types.
- `docs/verification/adapter-contracts.md`: read/write map contracts for adapter boundaries.
- `docs/verification/tcb.md`: dependencies and unproved logic outside the Verus target set.

## Rules

- Add new Rust business logic to `crates/verified-*`.
- If logic must stay outside the Verus target set, add an ID, reason, and alternate validation to `docs/verification/tcb.md`.
- Before adding branches to adapter code, confirm why the branch cannot be extracted into a pure function.
- Do not add fallback or shim branches that expand the unproved surface.

## Required Checks

```sh
cargo check --workspace
scripts/verify-verus.sh
```

`scripts/verify-verus.sh` enumerates `crates/verified-*/src/lib.rs` and verifies with `--no-cheating --cfg verus_keep_ghost`.

CI also runs `scripts/check_verification_policy.sh`. When Rust business logic changes under `crates/*/src/*.rs`, the PR body must cite either `verified_core::<function>` or a `TCB-<id>` entry.
