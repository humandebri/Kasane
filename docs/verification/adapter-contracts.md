# Adapter Contracts

Canister adapters own external APIs and stable-map operations. Business decisions follow `verified_core::*` results.

## `submit -> queue`

Reads:
- `seen_tx`
- `chain_state`
- `sender_expected_nonce`
- `pending_current_by_sender`
- `pending_by_sender_nonce`
- `pending_fee_index`
- `principal_pending_count`

Writes:
- `seen_tx`
- `tx_store`
- `eth_tx_hash_index`
- `queue_meta`
- `chain_state.next_queue_seq`
- `tx_locs`
- `pending_by_sender_nonce`
- `pending_meta_by_tx_id`
- `pending_current_by_sender`
- `pending_min_nonce`
- `principal_pending_count`
- `pending_fee_index`
- `pending_fee_key_by_tx_id`
- `ready_queue`
- `ready_key_by_tx_id`
- `ready_by_seq`

Detection:
- `debug_assert_queued_adapter_effects`
- `common::assert_runtime_indexes_match_pending`

## `produce -> persist`

Reads:
- `ready_queue`
- `ready_key_by_tx_id`
- `ready_by_seq`
- `tx_store`
- `chain_state`
- `head`
- `accounts`
- `storage`
- `codes`

Writes:
- `blocks`
- `tx_index`
- `receipts`
- `internal_traces`
- `tx_locs`
- `head`
- `chain_state`
- `pending_*`
- `ready_*`
- `metrics_state`
- `accounts`
- `storage`
- `codes`

Detection:
- `debug_assert_persisted_included_effects`
- `common::assert_block_persist_invariants`

## `drop`

Reads:
- `tx_store`
- `pending_meta_by_tx_id`
- `ready_key_by_tx_id`
- `pending_fee_key_by_tx_id`

Writes:
- `tx_store`
- `tx_locs`
- `dropped_ring`
- `pending_*`
- `ready_*`
- `eth_tx_hash_index`
- `metrics_state`

Detection:
- `debug_assert_dropped_payload_effects`
- `common::assert_dropped_tx_purged`

## `rebuild`

Reads:
- `tx_store`
- `pending_by_sender_nonce`

Writes:
- `principal_pending_count`
- `pending_fee_index`
- `pending_fee_key_by_tx_id`
- `ready_by_seq`
- `eth_tx_hash_index`

Detection:
- `common::assert_runtime_indexes_match_pending`
- `verify_eth_tx_hash_index`

## `prune`

Reads:
- `head`
- `blocks`
- `receipts`
- `tx_index`
- `tx_locs`
- `seen_tx`
- `prune_state`
- `prune_journal`

Writes:
- `blocks`
- `receipts`
- `tx_index`
- `tx_locs`
- `seen_tx`
- `prune_state`
- `prune_journal`
- `blob_store`

Detection:
- `phase1_prune`
- `prune_journal`
