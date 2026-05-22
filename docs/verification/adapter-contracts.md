# Adapter Contracts

canister adapterは外部APIとstable map操作だけを担当する。
業務判定は `verified_core::*` の結果に従う。

## submit -> queue

読む:
- `seen_tx`
- `chain_state`
- `sender_expected_nonce`
- `pending_current_by_sender`
- `pending_by_sender_nonce`
- `pending_fee_index`
- `principal_pending_count`

書く:
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

検出:
- `debug_assert_queued_adapter_effects`
- `common::assert_runtime_indexes_match_pending`

## produce -> persist

読む:
- `ready_queue`
- `ready_key_by_tx_id`
- `ready_by_seq`
- `tx_store`
- `chain_state`
- `head`
- `accounts`
- `storage`
- `codes`

書く:
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

検出:
- `debug_assert_persisted_included_effects`
- `common::assert_block_persist_invariants`

## drop

読む:
- `tx_store`
- `pending_meta_by_tx_id`
- `ready_key_by_tx_id`
- `pending_fee_key_by_tx_id`

書く:
- `tx_store`
- `tx_locs`
- `dropped_ring`
- `pending_*`
- `ready_*`
- `eth_tx_hash_index`
- `metrics_state`

検出:
- `debug_assert_dropped_payload_effects`
- `common::assert_dropped_tx_purged`

## rebuild

読む:
- `tx_store`
- `pending_by_sender_nonce`

書く:
- `principal_pending_count`
- `pending_fee_index`
- `pending_fee_key_by_tx_id`
- `ready_by_seq`
- `eth_tx_hash_index`

検出:
- `common::assert_runtime_indexes_match_pending`
- `verify_eth_tx_hash_index`

## prune

読む:
- `head`
- `blocks`
- `receipts`
- `tx_index`
- `tx_locs`
- `seen_tx`
- `prune_state`
- `prune_journal`

書く:
- `blocks`
- `receipts`
- `tx_index`
- `tx_locs`
- `seen_tx`
- `prune_state`
- `prune_journal`
- `blob_store`

非対象:
- `accounts`
- `storage`
- `codes`
- state root/trie storage

検出:
- `phase1_prune`
- `prune_journal`
