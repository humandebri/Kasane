# EVM Canister Traceability

This file links canister-level spec sections to `specgen` extracts and
`verified_core` references. Core safety raw targets have `draft`, `scenarios`,
review output, contract terms, and linked test evidence; `accept`,
`apply-contract`, `gen-verus`, and `verify` require a clean source anchor.

## specgen Extracts

| Spec area | Function | Extract |
| --- | --- | --- |
| Transaction submission | `evm_core::chain::submit_tx_in` | `spec/runs/submit_tx_in-1aa3e067/extract.json` |
| Transaction submission | `evm_core::chain::submit_tx` | `spec/runs/submit_tx-7dc9f82e/extract.json` |
| IC synthetic submission | `evm_core::chain::submit_ic_tx_input` | `spec/runs/submit_ic_tx_input-f634c1cd/extract.json` |
| Block production | `evm_core::chain::produce_block` | `spec/runs/produce_block-71d69281/extract.json` |
| Nonce view | `evm_core::chain::expected_nonce_for_sender_view` | `spec/runs/expected_nonce_for_sender_view-b0943c29/extract.json` |
| Block lookup | `evm_core::chain::get_block` | `spec/runs/get_block-875c0086/extract.json` |
| Receipt lookup | `evm_core::chain::get_receipt` | `spec/runs/get_receipt-b220f0a7/extract.json` |
| Pruning | `evm_core::chain::prune_blocks` | `spec/runs/prune_blocks-4451e57b/extract.json` |
| Queue visibility | `evm_core::chain::get_queue_snapshot` | `spec/runs/get_queue_snapshot-66b65e82/extract.json` |
| Nonce state | `evm_core::tx_submit::expected_nonce_for_sender` | `spec/runs/expected_nonce_for_sender-362008e4/extract.json` |
| Replacement policy | `evm_core::tx_submit::apply_nonce_and_replacement` | `spec/runs/apply_nonce_and_replacement-9ee01bf0/extract.json` |
| Nonce pure rule | `verified_core::nonce::classify_nonce` | `spec/runs/classify_nonce-3dada50d/extract.json` |
| Block instruction stop | `verified_core::block::should_stop_execution` | `spec/runs/should_stop_execution-207b8917/extract.json` |
| Instruction budget | `verified_core::block::remaining_instruction_budget` | `spec/runs/remaining_instruction_budget-77147f7b/extract.json` |
| Block gas budget | `verified_core::block::tx_fits_block_gas` | `spec/runs/tx_fits_block_gas-a077863a/extract.json` |
| Core safety model | `verified_core::core_safety::submit_transition_safe_raw` | `spec/runs/submit_transition_safe_raw-3a7d7873/extract.json` |
| Core safety model | `verified_core::core_safety_included::included_tx_safe_raw` | `spec/runs/included_tx_safe_raw-8883376d/extract.json` |
| Core safety model | `verified_core::core_safety_block::block_commit_safe_raw` | `spec/runs/block_commit_safe_raw-318a0bf6/extract.json` |
| Pruning safety model | `verified_core::prune_safety::block_is_prunable` | `spec/runs/block_is_prunable-04224fd7/extract.json` |
| Pruning safety model | `verified_core::prune_safety::block_is_retained` | `spec/runs/block_is_retained-9d9115e5/extract.json` |
| Pruning safety model | `verified_core::prune_safety::prune_boundary_safe` | `spec/runs/prune_boundary_safe-77bde266/extract.json` |
| Pruning safety model | `verified_core::prune_safety::prune_tx_cleanup_complete` | `spec/runs/prune_tx_cleanup_complete-171d1899/extract.json` |

## Canister Entrypoint Sources

| Spec area | Source |
| --- | --- |
| Public Candid API | `crates/ic-evm-gateway/evm_canister.did` |
| Gateway entrypoints | `crates/ic-evm-gateway/src/lib.rs` |
| Core chain behavior | `crates/evm-core/src/chain.rs` |
| Transaction nonce adapter | `crates/evm-core/src/tx_submit.rs` |
| Stable state model | `crates/evm-db/src/stable_state.rs` |

## Verification References

| Rule | Reference |
| --- | --- |
| Nonce ordering precedes replacement checks. | `verified_core::nonce::classify_nonce` |
| Low incoming nonce returns `TooLow`. | `verified_core::nonce::classify_nonce` |
| Gap incoming nonce returns `Gap`. | `verified_core::nonce::classify_nonce` |
| Equal replacement price returns conflict. | `verified_core::nonce::classify_nonce` |
| Strictly higher replacement price returns replace. | `verified_core::nonce::classify_nonce` |
| Block production stops on instruction or policy budget. | `verified_core::block::should_stop_execution` |
| Remaining instruction budget saturates safely. | `verified_core::block::remaining_instruction_budget` |
| Transaction gas inclusion respects block gas limit. | `verified_core::block::tx_fits_block_gas` |
| Accepted submit transition writes current pending and queued location evidence. | `verified_core::core_safety::submit_transition_safe_raw` |
| Included transaction has matching location, receipt, and index evidence. | `verified_core::core_safety_included::included_tx_safe_raw` |
| Block commit has strict nonterminal head progress, gas, and batch-count evidence. | `verified_core::core_safety_block::block_commit_safe_raw` |
| Pruning only crosses blocks at or before the retention boundary. | `verified_core::prune_safety::block_is_prunable` |
| Retained blocks are nonfuture blocks outside the prunable range. | `verified_core::prune_safety::block_is_retained` |
| Pruned boundary is unset or monotonically advances without entering retained range. | `verified_core::prune_safety::prune_boundary_safe` |
| Pruned transaction cleanup removes receipt, tx index, tx loc, seen-tx, tx store, and internal traces from observation. | `verified_core::prune_safety::prune_tx_cleanup_complete` |

## Adapter Evidence

| Boundary | Evidence |
| --- | --- |
| nonce replacement adapter | `crates/evm-core/tests/phase1_nonce_sequence.rs::replacement_requires_higher_effective_fee` is linked as test evidence for `submit_transition_safe_raw`; it proves same-nonce lower/equal replacement is rejected, strict higher replacement wins, the old tx is dropped, and only the replacement is included |
| block persistence adapter | `crates/evm-core/tests/common/mod.rs::assert_block_persist_invariants` is linked as test evidence for `included_tx_safe_raw` and `block_commit_safe_raw`; it proves included tx ids have matching receipt, tx index, tx loc, no pending/ready refs, and block persistence invariants |
| gateway submit/receipt adapter | `crates/ic-evm-gateway/src/tests.rs::gateway_submit_ic_tx_adapter_preserves_queue_and_receipt_invariants` is linked through `spec/adapter-evidence.toml` and the accepted test evidence for `submit_transition_safe_raw`, `included_tx_safe_raw`, and `block_commit_safe_raw`; it proves DTO parsing plus gateway submit helper writes queued location, pending receipt status, included location, receipt, tx index, and monotonic head after block production |
| pruning adapter | `crates/evm-core/tests/phase1_prune.rs` and `crates/evm-core/tests/prune_journal.rs` are linked as test evidence for the pruning safety targets; they prove old block/receipt/index/location deletion, retained range preservation, `max_ops` bounding, journal recovery, and idempotency |
| gateway pruning query adapter | `crates/ic-evm-gateway/src/tests.rs` pruned/receipt lookup tests are linked through `spec/adapter-evidence.toml`; they prove pruned blocks and receipts return `Pruned`, unknown tx with prune boundary returns `PossiblyPruned` through RPC status lookup, and retained receipts remain queryable |

## Review Gaps

- `specgen status --check` for `submit_transition_safe_raw`,
  `included_tx_safe_raw`, and `block_commit_safe_raw` currently has no drift reasons;
  it fails only because the targets were extracted from a dirty worktree while
  this change is still uncommitted.
- Adapter test functions are not specgen targets because they are unit-return
  integration checks with many runtime dependencies. Production adapter helpers
  that are intentionally covered by pure-model test evidence are listed in
  `spec/adapter-evidence.toml`.
- Pruning proof excludes stable memory, StableBTreeMap, blob reclaim physical
  reuse, IC trap/crash persistence, and OS/process behavior. These remain trust
  boundaries; the proof covers the pure boundary and observation invariants plus
  adapter evidence.
- `specgen status --check` passes for `submit_tx_in` and
  `should_stop_execution`. Most adapter/core extracts currently report
  unresolved dependencies because `specgen extract` is function-local and the
  targets call repo-local helpers, stable maps, and verified-core utilities.
  These failures are recorded as extraction limits, not as canister spec
  acceptance failures.
- Wrap and native ledger workflows need function-level specgen targets in a
  follow-up because their behavior crosses async ledger calls and request state.
- RPC query semantics need additional extracts from `crates/ic-evm-rpc/src/lib.rs`
  before they can be accepted as function-level specs.
- Upgrade recovery needs focused scenarios around `pre_upgrade`, `post_upgrade`,
  wrap worker recovery, and unwrap dispatch recovery before acceptance.
