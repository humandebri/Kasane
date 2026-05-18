# Verus TCB Ledger

This document tracks assumptions outside the Verus proof target set. When Rust business logic is added outside the proof target set, record the reason and alternate validation here.

## Proof Targets

- `crates/verified-*`: pure state transitions, bound checks, codec boundaries, pruning calculations, and state-diff application decisions.
- `crates/verified-*/src`: implementation functions called directly by adapters, with `requires`, `ensures`, and `invariant` specifications.

## TCB

| ID | Area | Assumption | Alternate validation |
| --- | --- | --- | --- |
| `TCB-revm` | `revm` | EVM execution semantics, gas use, and halt reasons match upstream behavior. | compatibility E2E, upstream `revm` tests, fixed feature checks |
| `TCB-alloy` | `alloy-*` | RLP, signatures, and Ethereum type decode/encode match the expected specs. | existing unit/integration tests, RPC compatibility smoke |
| `TCB-keccak` | `keccak` | hash implementation is Ethereum-compatible. | known vector tests, state-root tests |
| `TCB-dfinity` | DFINITY crates | `ic-cdk`, `ic-stable-structures`, Candid, and timers follow their public contracts. | PocketIC, upgrade smoke, deployment smoke |
| `TCB-ic-runtime` | IC runtime | caller, time, cycles, performance counters, and stable memory follow IC runtime behavior. | local/mainnet smoke, operational monitoring |
| `TCB-typescript` | TypeScript tools | explorer, indexer, gateway, and UI code are outside the Verus target set. | TypeScript checks, npm tests, E2E |
| `TCB-github-actions` | GitHub Actions | pinned Verus release assets and Rust toolchains are fetched successfully. | `scripts/verify-verus.sh` and CI logs |

## Rules

- Add new Rust business logic to `crates/verified-*`.
- If logic is assigned to the TCB, add an ID, assumption, and alternate validation.
- Adapter code should only call IC APIs, stable memory, time, cycles, Candid, and `revm`.
- Do not expand the unproved branch surface with fallback or shim logic.
