# EVM Canister Scenario Review Draft

Each scenario starts as `needs_review`. Human review should mark it accepted,
rejected, or documented before this canister spec becomes an accepted baseline.

## Transaction Submission

| ID | Scenario | Expected behavior | Status |
| --- | --- | --- | --- |
| tx-ic-accepted | Valid IC synthetic transaction is submitted by a non-anonymous caller. | Returns tx id, inserts pending state, schedules mining. | needs_review |
| tx-eth-accepted | Valid signed raw Ethereum transaction is submitted. | Returns tx id and schedules mining. | needs_review |
| tx-duplicate | A previously seen transaction id is submitted again. | Rejects as already seen. | needs_review |
| tx-too-large | Transaction bytes exceed `MAX_TX_SIZE`. | Rejects as too large. | needs_review |
| tx-nonce-low | Incoming sender nonce is below expected nonce. | Rejects as nonce too low before replacement checks. | needs_review |
| tx-nonce-gap | Incoming sender nonce is above expected nonce. | Rejects as nonce gap before replacement checks. | needs_review |
| tx-replace-conflict | Incoming tx matches expected nonce but fee is equal or lower. | Rejects as nonce conflict. | needs_review |
| tx-replace-ok | Incoming tx matches expected nonce and fee is strictly higher. | Replaces current pending tx. | needs_review |

## Block Production

| ID | Scenario | Expected behavior | Status |
| --- | --- | --- | --- |
| block-empty | No executable pending transaction exists. | Returns queue-empty or no-executable outcome without advancing committed head. | needs_review |
| block-include | Pending tx fits gas and instruction budget. | Includes tx, persists receipt, updates indexes, advances head. | needs_review |
| block-gas-stop | Next tx would exceed block gas limit. | Stops inclusion before exceeding limit. | needs_review |
| block-instruction-stop | Instruction soft limit is exhausted. | Stops production and keeps persisted state coherent. | needs_review |
| block-exec-failed | EVM execution fails after inclusion decision. | Stores deterministic receipt status and fee/gas data. | needs_review |

## RPC and Query

| ID | Scenario | Expected behavior | Status |
| --- | --- | --- | --- |
| query-head | `rpc_eth_block_number` is called. | Returns current committed head number. | needs_review |
| query-missing-block | Unknown block number is requested. | Returns not found shape. | needs_review |
| query-pruned-block | Pruned block number is requested through status API. | Returns pruned status with boundary. | needs_review |
| query-balance | Account balance query is called with valid address and tag. | Returns encoded balance bytes. | needs_review |
| query-storage-missing | Missing storage slot is queried. | Returns zero value. | needs_review |
| query-bad-input | Address, slot, or tx id length is invalid. | Returns structured RPC or lookup error. | needs_review |

## Wrap and Native Flows

| ID | Scenario | Expected behavior | Status |
| --- | --- | --- | --- |
| wrap-quote | Allowed asset and valid recipient are quoted. | Returns fee and gas quote. | needs_review |
| wrap-disallowed-asset | Disallowed asset is quoted or submitted. | Rejects before fee collection. | needs_review |
| wrap-submit | Valid wrap request is submitted. | Collects fee, records request, enqueues worker. | needs_review |
| wrap-idempotent | Existing wrap request is submitted again. | Returns existing response without duplicate queue state. | needs_review |
| native-credit-auth | `credit_native_deposit` is called by non-wrap canister. | Rejects as unauthorized. | needs_review |
| unwrap-dispatch | Valid unwrap dispatch request is sent. | Records or dispatches request according to current state. | needs_review |
| retry-terminal | Retry is requested for terminal request. | Does not mutate terminal outcome. | needs_review |
| repair-stale | Repair is run with stale in-progress operations. | Requeues or marks only recoverable operations. | needs_review |

## Operations and Upgrade

| ID | Scenario | Expected behavior | Status |
| --- | --- | --- | --- |
| ops-status | Ops status is queried. | Reports config, migration, cycle, and error counters. | needs_review |
| prune-config-invalid | Prune policy has too small max ops. | Rejects validation. | needs_review |
| prune-work | `prune_blocks` is called with valid retain and max ops. | Performs bounded prune work and reports remaining state. | needs_review |
| upgrade-preserve | Canister upgrades after committed blocks exist. | Preserves blocks, receipts, indexes, account state, and config. | needs_review |
| upgrade-worker-recovery | Upgrade occurs with active wrap or unwrap requests. | Recovers queues without duplicates and quarantines decode failures. | needs_review |
