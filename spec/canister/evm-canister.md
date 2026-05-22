# EVM Canister Specification Draft

## Background

The EVM canister exposes Kasane's EVM-compatible transaction, block, RPC, wrap,
native deposit, operations, and pruning surfaces through the Candid service in
`crates/ic-evm-gateway/evm_canister.did`.

The canister entrypoint layer lives in `crates/ic-evm-gateway`, delegates core
chain behavior to `crates/evm-core`, and stores persistent chain data through
`crates/evm-db`. Function-level evidence is tracked with `specgen` artifacts
under `spec/runs/`.

## Purpose

This document is a review draft for canister-level intended behavior. It does
not replace Verus contracts. Verus should stay attached to pure Rust logic such
as `verified_core::<function>` and extracted `evm-core` functions.

The draft separates externally observable API behavior from lower-level
verification targets so reviewers can decide which rules become accepted specs.

## Public API Groups

### Transaction Submission

- `submit_ic_tx`
- `rpc_eth_send_raw_transaction`
- `estimate_ic_tx`
- `expected_nonce_by_address`
- `get_pending`
- `get_queue_snapshot`

### Blocks, Receipts, and Export

- `get_block`
- `get_receipt`
- `export_blocks`
- `rpc_eth_block_number`
- `rpc_eth_get_block_by_number`
- `rpc_eth_get_block_by_number_with_status`
- `rpc_eth_get_block_number_by_hash`
- `rpc_eth_get_transaction_by_eth_hash`
- `rpc_eth_get_transaction_by_tx_id`
- `rpc_eth_get_transaction_receipt_by_eth_hash`
- `rpc_eth_get_transaction_receipt_with_status_by_eth_hash`
- `rpc_eth_get_transaction_receipt_with_status_by_tx_id`

### Ethereum RPC Queries

- `rpc_eth_chain_id`
- `rpc_eth_get_balance`
- `rpc_eth_get_code`
- `rpc_eth_get_storage_at`
- `rpc_eth_call_object`
- `rpc_eth_call_object_at`
- `rpc_eth_call_rawtx`
- `rpc_eth_estimate_gas_object`
- `rpc_eth_estimate_gas_object_at`
- `rpc_eth_fee_history`
- `rpc_eth_gas_price`
- `rpc_eth_max_priority_fee_per_gas`
- `rpc_eth_get_logs_paged`
- `rpc_eth_get_transaction_count_at`
- `rpc_eth_history_window`

### Wrap and Native Flows

- `quote_wrap_request`
- `submit_wrap_request`
- `get_wrap_runtime_config`
- `get_allowed_assets`
- `set_allowed_assets`
- `get_fee_policy`
- `set_fee_policy`
- `quote_native_deposit`
- `submit_native_deposit`
- `credit_native_deposit`
- `quote_native_withdrawal`
- `dispatch_native_withdrawal_request`
- `get_native_deposit_result`
- `get_request`
- `retry_request`
- `retry_native_deposit`
- `retry_native_withdrawal`
- `recover_failed_wrap`
- `repair_stale_wrap_operations`
- `get_unwrap_requirements`
- `dispatch_unwrap_request`
- `get_unwrap_dispatch_overview`
- `get_unwrap_request_ids_by_tx_id`
- `get_unwrap_request_ids_by_eth_tx_hash`

### Operations, Metrics, and Pruning

- `health`
- `get_ops_status`
- `get_cycle_balance`
- `metrics`
- `metrics_prometheus`
- `memory_breakdown`
- `set_log_filter`
- `get_prune_status`
- `set_prune_policy`
- `set_pruning_enabled`
- `prune_blocks`

### Standards and Consent

- `icrc10_supported_standards`
- `icrc21_canister_call_consent_message`

## State Model

The canister state is stored in stable structures initialized by
`evm_db::stable_state::init_stable_state`.

Core state:

- account balances, nonces, contract code, and storage
- submitted transaction bytes and seen transaction ids
- blocks, receipts, transaction indexes, and transaction locations
- ready queue, pending sender queues, fee indexes, and per-principal pending counts
- wrap requests, unwrap dispatch requests, native credit records, and related queues
- pruning config, pruning state, prune journal, and dropped transaction ring
- ops state, ops metrics, runtime config, and schema migration state
- state-root metadata, node database, storage roots, mismatch records, and GC state

State invariants for review:

- The head block number is monotonic.
- A transaction id maps to at most one included location.
- A sender has at most one current pending transaction for the expected nonce.
- Pending replacement must not bypass nonce ordering.
- Query methods must not mutate stable state.
- Upgrade must preserve committed blocks, receipts, indexes, account state, and config.

## Safety Proof Scope

The core safety claim is limited to the lifecycle
`submit -> pending -> produce_block -> receipt/index/head update`.

Under correct adapter observations and external-boundary assumptions, the
verified model must preserve:

- nonce safety: low/gap nonces are rejected, and same-nonce replacement requires
  a strictly higher effective gas price
- pending uniqueness: each sender has at most one current pending transaction,
  and a replaced transaction is removed from execution candidates
- block safety: nonterminal produced block head advances by exactly one, and
  included transactions do not exceed configured block gas evidence
- receipt/index consistency: every included transaction has one matching
  receipt, index entry, and included location

Verus proves only the pure `verified_core` transition predicates. `evm-core`
and `ic-evm-gateway` are adapters: their evidence is tests plus `specgen` gates
showing observed state follows the pure model.

Trust boundaries outside this proof:

- revm execution semantics
- state root/trie correctness and current account-state equivalence
- IC runtime scheduling and query/update execution model
- stable memory implementation
- ledger and other external canister calls
- OS, process, build toolchain, and host filesystem behavior

Pruning proof scope is narrower than full state correctness. It claims old block
history, receipts, indexes, locations, seen-tx markers, tx store records, and
internal traces become consistently unobservable across the pruned boundary. It
does not prove revm state transition validity, account trie correctness, or
state root correctness. Adapter evidence only checks that pruning does not use
the current account/state-root storage path as part of history deletion.

## Transaction Lifecycle

`submit_ic_tx` and `rpc_eth_send_raw_transaction` normalize input into
`evm_core::chain::TxIn`, then submit through `submit_tx_in`.

Lifecycle:

1. Reject anonymous or write-disabled calls where required.
2. Decode or normalize the transaction.
3. Derive the sender and transaction id.
4. Reject already-seen or oversized transactions.
5. Validate fee and nonce policy.
6. Insert or replace pending transaction state.
7. Schedule mining after accepted update calls.
8. Include executable transactions during block production.
9. Persist block, receipt, indexes, and transaction location.

Failure semantics:

- malformed input returns decode or invalid argument errors
- nonce below expected returns low-nonce error
- nonce above expected returns gap error
- equal or lower replacement price returns conflict
- queue or sender caps return capacity errors
- execution failure is recorded as a receipt-level result when inclusion occurs

## Block Production

`produce_block` selects pending executable transactions, executes them, persists
the block, and advances indexes. It must respect transaction count limits,
instruction soft limits, and block gas limits. `block_gas_limit == 0` is the
existing pure-model sentinel for disabled gas limiting.

Block invariants for review:

- included transaction indexes are monotonic within a block
- block gas used must not exceed the configured block gas limit
- block production stops when the instruction or gas budget is exhausted
- dropped transactions are observable through metrics or pending status
- persisted receipts correspond to included transaction ids

## RPC and Query Behavior

RPC query methods expose chain data without mutating state. Block-tagged queries
must resolve `latest`, explicit block numbers, and pruned ranges consistently.

Query behavior for review:

- missing blocks and receipts return the documented not-found shape
- pruned data is distinguishable from never-existing data where status APIs exist
- storage reads for missing slots return zero values
- malformed address, slot, or transaction inputs return structured RPC errors
- query instruction soft limits are enforced by query execution paths

## Wrap and Native Deposit/Withdraw Flows

Wrap and native flows are integrated canister workflows around EVM execution and
ICRC ledger interactions.

Rules for review:

- quote methods validate asset allow-list and argument shape before returning fees
- submit methods reject anonymous callers and write-disabled mode
- request ids provide idempotence for active or completed requests
- fee collection precedes queueing for wrap submissions
- dispatch and retry methods preserve terminal request states
- recovery methods requeue only recoverable failed or stale operations
- unwrap lookup methods map EVM transaction logs back to request ids

## Operations, Pruning, and Metrics

Operational methods expose health, metrics, memory, queue, and pruning state.
Control-plane update methods must require authorized writes.

Pruning rules for review:

- pruning configuration validates minimum operation limits
- `prune_blocks` advances pruning work without deleting retained head data
- pruned block ranges are reported through lookup/export status
- prune journal recovery must complete before new destructive prune work

## Upgrade and Stable-State Behavior

`pre_upgrade` and `post_upgrade` use `evm_db::upgrade` plus stable-state
initialization. Upgrade arguments are required by the gateway upgrade path.

Upgrade rules for review:

- committed chain data persists across upgrade
- pending mempool state may be rebuilt or cleared only by explicit migration logic
- schema migration state is observable through ops status
- wrap and unwrap worker state is recovered without duplicate queue entries
- decode-failed unwrap requests can be quarantined rather than dispatched

## Verification Links

Current function-level verification evidence is listed in
`spec/canister/evm-canister-traceability.md`.

Primary verified-core references:

- `verified_core::nonce::classify_nonce`
- `verified_core::block::should_stop_execution`
- `verified_core::block::remaining_instruction_budget`
- `verified_core::block::tx_fits_block_gas`
- `verified_core::core_safety::submit_transition_safe`
- `verified_core::core_safety::included_tx_safe`
- `verified_core::core_safety::block_commit_safe`

## Review Status

This file is a canister-level draft. It is not accepted. Review must confirm the
scenarios in `spec/canister/evm-canister-scenarios.md` before any downstream
accepted spec or Verus contract is generated.
