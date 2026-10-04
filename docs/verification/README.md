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

## Lean EVM Adapter Model

`proofs/evm` contains Lean 4 proofs for a mathematical model of fee arithmetic,
state-map updates, receipt projection, and commit/error ordering. Run
`bash scripts/verify-lean.sh`; see [scope and remaining obligations](../../proofs/evm/README.md).
It also proves balance/bound preservation in the projected journal, write-list
composition and replay idempotence, precommit rejection/retry safety, asset CALL
admission, and correspondence between the Lean journal and logical storage maps.
These model proofs do not remove `TCB-revm` or prove the Rust adapter equivalent
to the model. The command also audits proof axioms, detects source drift, and
compares boundary cases against the actual Rust pure functions. It is a separate
local gate; CI's `evm-proofs` job invokes it through `verify-revm.sh`.

`bash scripts/verify-revm.sh` additionally checks the pinned revm source/features,
compares actual journal traces with Lean, and runs official Prague state fixtures
and Kasane CALL/CREATE/SELFDESTRUCT and precompile rollback regressions.
The stable DB adapter skips untouched accounts, including a reverted CREATE target.
See [implementation correspondence](../../proofs/evm/revm-correspondence.md).

`bash scripts/verify-evm-proofs.sh` runs the Verus implementation contracts and
the Lean/revm gate together. The legacy gas-price property is proved independently
in Verus and Lean; the cross-language correspondence still relies on reviewed
argument mapping and finite execution tests.

## Canister Upgrade Validation

See [2026-09-30 upgrade and snapshot validation scope](mainnet-readiness-2026-09-30.md)
for the PocketIC state-preservation tests and the remaining deployed-version migration checks.
