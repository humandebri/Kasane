# RPC Gateway

Gateway-side implementation that translates canister Candid APIs into Ethereum-style JSON-RPC 2.0.

## Setup

```bash
cd tools/rpc-gateway
npm install
cp .env.example .env.local
```

At minimum, set `EVM_CANISTER_ID` in `.env.local`.

If you use update calls such as `eth_sendRawTransaction`, set a signing identity PEM as well.

```env
EVM_CANISTER_ID=aaaaa-aa
RPC_GATEWAY_IDENTITY_PEM="-----BEGIN PRIVATE KEY-----..."
```

Supported PEM formats are `secp256k1` and `ed25519 (PKCS#8)`. If `icp identity export` outputs key type `ec`, it is not usable here. Create a dedicated `secp256k1` key for the gateway.

## Run

```bash
npm run dev
```

Default endpoint: `http://127.0.0.1:8545`

## Supported Methods

- `GET /v2/x402/supported`
- `POST /v2/x402/verify`
- `POST /v2/x402/settle`
- `web3_clientVersion`
- `net_version`
- `eth_chainId`
- `eth_blockNumber`
- `eth_gasPrice`
- `eth_maxPriorityFeePerGas`
- `eth_feeHistory`
- `eth_syncing`
- `eth_getBlockByNumber`
- `eth_getTransactionByHash`
- `eth_getTransactionReceipt`
- `eth_getBalance` (accepts `latest/pending/safe/finalized/earliest/QUANTITY`)
- `eth_getTransactionCount` (accepts `latest/pending/safe/finalized/earliest/QUANTITY`)
- `eth_getCode` (accepts `latest/pending/safe/finalized/earliest/QUANTITY`)
- `eth_getStorageAt` (accepts `latest/pending/safe/finalized/earliest/QUANTITY`)
- `eth_getLogs` (with limitations)
- `eth_call(callObject, blockTag)` (accepts `latest/pending/safe/finalized/earliest/QUANTITY`)
- `eth_estimateGas(callObject, blockTag)` (accepts `latest/pending/safe/finalized/earliest/QUANTITY`)
- `eth_sendRawTransaction`

## Support Summary

| Category | Methods |
| --- | --- |
| Supported | `web3_clientVersion`, `net_version`, `eth_chainId`, `eth_blockNumber`, `eth_gasPrice`, `eth_maxPriorityFeePerGas`, `eth_feeHistory`, `eth_syncing`, `eth_getBlockByNumber`, `eth_getTransactionByHash`, `eth_getTransactionReceipt`, `eth_getBalance`, `eth_getTransactionCount`, `eth_getCode`, `eth_getStorageAt`, `eth_getLogs`, `eth_call`, `eth_estimateGas`, `eth_sendRawTransaction` |
| Not supported | `eth_getBlockByHash`, `eth_getTransactionByBlockHashAndIndex`, `eth_getTransactionByBlockNumberAndIndex`, `eth_getBlockTransactionCountByHash`, `eth_getBlockTransactionCountByNumber`, `eth_newFilter`, `eth_getFilterChanges`, `eth_uninstallFilter`, `eth_subscribe`, `eth_unsubscribe`, `eth_pendingTransactions` |

Note: some methods in `Supported` are still partial. See the compatibility table below.

## x402 Facilitator

The gateway also exposes a minimal x402 v2 facilitator for Kasane exact EVM payments.

```env
X402_NETWORK=eip155:4801360
X402_RPC_URL=http://127.0.0.1:8545
X402_SETTLER_PRIVATE_KEY=0x...
```

- `/v2/x402/supported` advertises `x402Version=2`, `scheme=exact`, and `network=eip155:4801360` by default.
- `/v2/x402/verify` validates payment shape, token `name()` / `version()`, nonce state, and simulates `receiveWithAuthorization`.
- `/v2/x402/settle` signs and submits `receiveWithAuthorization` through `X402_RPC_URL`.
- `X402_SETTLER_PRIVATE_KEY` must be the key for the merchant `payTo` address.
- Wrapped tokens use EIP-3009 domain version `"1"`, so payment requirements must include `extra.version = "1"`.

## Compatibility Matrix (canister ↔ gateway)

This table is the public compatibility window. To keep the mirror repo self-contained, the API compatibility baseline is defined under this directory.

### Gateway Local API Compatibility Baseline
- `contracts/gateway-api-compat-baseline.did`
- `contracts/gateway-api-compat-methods.txt`
- Supplement: `contracts/README.md`

### Canonical Source
- `contracts/*` (this directory)
- Guard script: `scripts/check_gateway_api_compat_baseline.sh`

| api_baseline_version | gateway_version | status | notes |
| --- | --- | --- | --- |
| `v1` | `ic-evm-rpc-gateway@0.1.x` | supported | compatible as long as `scripts/check_gateway_api_compat_baseline.sh` passes |

Notes:
- Adding methods outside the baseline is allowed.
- Removing/changing types/changing `query`↔`update` for baseline methods is a breaking change.
- When baseline content changes, update `contracts/*` and this matrix in the same PR.

## callObject Coverage

- Supported fields: `to`, `from`, `gas`, `gasPrice`, `value`, `data`, `nonce`, `maxFeePerGas`, `maxPriorityFeePerGas`, `chainId`, `type`, `accessList`
- `type` accepts only `0x0` and `0x2`
- `accessList` accepts EIP-2930 format (`address`, `storageKeys[]`)
- If `nonce` is omitted, canister-side current nonce for `from` is used
- Unsupported fields return `-32602 invalid params`
- Validation rules:
  - `gasPrice` cannot be combined with `maxFeePerGas` / `maxPriorityFeePerGas`
  - `maxPriorityFeePerGas` requires `maxFeePerGas`
  - `maxPriorityFeePerGas <= maxFeePerGas`
  - `type=0` cannot be combined with `max*`
  - `type=2` cannot be combined with `gasPrice`

## Ethereum JSON-RPC Compatibility Details

This section reflects the current implementation and is treated as the canonical compatibility detail. If this table changes, update the summary table in root README in the same PR.

| Method | Status | Current behavior | Limitation | Alternative/Note |
| --- | --- | --- | --- | --- |
| `eth_chainId` | Supported | Returns canister `rpc_eth_chain_id` | None | `net_version` returns the same value in decimal string |
| `eth_blockNumber` | Supported | Returns canister `rpc_eth_block_number` | None | - |
| `eth_gasPrice` | Partially supported | Returns canister `rpc_eth_gas_price` (`max(base_fee + max(estimated_priority,min_priority), min_gas_price)`) | `-32000 state unavailable` when observation data is insufficient | Uses the same `min_priority_fee` floor as `eth_maxPriorityFeePerGas` to stay aligned with acceptance rules |
| `eth_maxPriorityFeePerGas` | Partially supported | Returns canister `rpc_eth_max_priority_fee_per_gas` (`max(estimated_priority, min_priority_fee)`) | `-32000 state unavailable` when observation data is insufficient | Simplified EIP-1559 estimate with acceptance-rule floor |
| `eth_feeHistory` | Partially supported | Returns canister `rpc_eth_fee_history` | `blockCount` accepts number / QUANTITY(hex) / decimal string, max 256. `pending` currently behaves as `latest` | reward is estimated with gasUsed weight |
| `eth_syncing` | Supported | Always returns `false` | Sync progress object is not supported | Designed for immediate execution model |
| `eth_getBlockByNumber` | Partially supported | Resolves `blockTag` and returns block | `latest/pending/safe/finalized` are treated as head. Pruned range returns `-32001` | canister method: `rpc_eth_get_block_by_number_with_status` |
| `eth_getTransactionByHash` | Supported | Looks up by `eth_tx_hash` | No direct `tx_id` lookup. During unfinished migration / critical corruption returns `-32000 state unavailable` | canister method: `rpc_eth_get_transaction_by_eth_hash` |
| `eth_getTransactionReceipt` | Partially supported | Looks up receipt by `eth_tx_hash` | If `Found.transactionHash` does not match requested hash, returns `null` (misdelivery protection). During unfinished migration / critical corruption returns `-32000`, pruned range returns `-32001` | canister method: `rpc_eth_get_transaction_receipt_with_status_by_eth_hash` |
| `eth_getBalance` | Partially supported | Returns balance | QUANTITY succeeds only when equal to `head`; lower than `head` returns `exec.state.unavailable`, out-of-window returns `invalid.block_range.out_of_window` | Maps canister `Err` to `-32602` / `-32000` |
| `eth_getTransactionCount` | Partially supported | Returns canister `rpc_eth_get_transaction_count_at(address, tag)` | `pending` returns pending nonce. `earliest` returns `exec.state.unavailable` because historical nonce is not provided (`oldest_available>0` becomes out-of-window). QUANTITY behaves the same as balance | `earliest` is evaluated as block `0` |
| `eth_getCode` | Partially supported | Returns bytecode | QUANTITY succeeds only when equal to `head`; lower than `head` returns `exec.state.unavailable`, out-of-window returns `invalid.block_range.out_of_window` | Maps canister `Err` to `-32602` / `-32000` |
| `eth_getStorageAt` | Partially supported | Returns storage value | QUANTITY succeeds only when equal to `head`; lower than `head` returns `exec.state.unavailable`, out-of-window returns `invalid.block_range.out_of_window` | `slot` accepts both QUANTITY and DATA(32bytes) |
| `eth_getLogs` | Partially supported | Collects via `rpc_eth_get_logs_paged` (topic0 OR arrays are expanded/merged in gateway) | only one `address`, `topics[1+]` unsupported. `blockHash` is resolved by scanning latest `RPC_GATEWAY_LOGS_BLOCKHASH_SCAN_LIMIT` blocks (default `2000`) | oversized ranges return `-32005 limit exceeded` |
| `eth_call` | Partially supported | Delegates `callObject + tag` to canister `rpc_eth_call_object_at` | QUANTITY succeeds only when equal to `head`; lower than `head` returns `exec.state.unavailable`, out-of-window returns `invalid.block_range.out_of_window` | revert maps to `-32000` + `error.data` |
| `eth_estimateGas` | Partially supported | Delegates `callObject + tag` to canister `rpc_eth_estimate_gas_object_at` | QUANTITY succeeds only when equal to `head`; lower than `head` returns `exec.state.unavailable`, out-of-window returns `invalid.block_range.out_of_window` | Maps canister `Err` to `-32602` / `-32000` |
| `eth_sendRawTransaction` | Supported | Delegates raw tx to canister submit API, resolves returned `tx_id` into `eth_tx_hash`, and returns `0x...` | submit failures map to JSON-RPC errors. If `eth_tx_hash` cannot be resolved returns `-32000` | canister method: `rpc_eth_send_raw_transaction` |
| `eth_newFilter` / `eth_getFilterChanges` / `eth_uninstallFilter` | Not supported | Filter APIs are not implemented | Out of current scope | use `rpc_eth_get_logs_paged` |
| `eth_subscribe` / `eth_unsubscribe` | Not supported | WebSocket subscription is not implemented | Out of current scope | use `eth_blockNumber` polling |
| pending / mempool APIs (for example `eth_pendingTransactions`) | Not supported | pending/mempool concept is not provided | Out of current scope | track with post-submit block production and query methods |

This compatibility table covers JSON-RPC behavior only. Opcode execution semantics differences are out of scope for now.

Operational notes (current implementation):
- Pruning: canister prunes history, so old ranges can return `Pruned` / `PossiblyPruned` in `rpc_eth_get_block_by_number_with_status` / `rpc_eth_get_transaction_receipt_with_status_by_eth_hash`.
- Timer-driven mining: mining runs via one-shot `set_timer` re-scheduled every tick. `mining_scheduled` prevents duplicate scheduling.
- Timer-driven mining details: only automatic mining is provided. When `ready_queue` is empty, only next scheduling is performed.
- Timer-driven stop conditions: on mining failure, retries happen at base interval. During cycle-critical or migration, writes are rejected and mining stops; cycle observer tick (60s) helps re-scheduling after recovery. Prune is attempted only on block events (`block_number % 84 == 0`).
- Submit/execute split: `eth_sendRawTransaction` delegates only to submit API; execution finalization is a later phase (block production).
- Monitoring: do not treat `eth_sendRawTransaction` success as final success. Use `eth_getTransactionReceipt.status == 0x1` (`0x0` means execution failure).
- `eth_sendRawTransaction` return value: gateway resolves canister `rpc_eth_send_raw_transaction` returned `tx_id` via `rpc_eth_get_transaction_by_tx_id`; unresolved result returns `-32000`.
- `eth_getTransactionReceipt.logs[].logIndex`: returned as block-wide sequential index.
- Hash semantics: canister stores `tx_id`; Ethereum-compatible lookup uses `eth_tx_hash`. Gateway maps `eth_*ByHash` to `eth_tx_hash` APIs.
- API split: receipt status APIs are split between `rpc_eth_get_transaction_receipt_with_status_by_eth_hash` (external) and `rpc_eth_get_transaction_receipt_with_status_by_tx_id` (internal). Gateway uses only the former.
- Finality assumptions: single-sequencer assumption; reorg-driven behavior is not provided.
- `expected_nonce_by_address` is a query method. When calling directly with `icp canister call`, omit `--query` and you will get `IC0406`.

Related constants (current values):
- mining base interval: `DEFAULT_MINING_INTERVAL_MS = 2_000`
- cycle observer interval: `60s` (`set_timer_interval(Duration::from_secs(60), ...)`)
- prune policy interval field: `DEFAULT_PRUNE_TIMER_INTERVAL_MS = 3_600_000` (internal stored value; currently unused as `set_prune_policy` input)
- prune event interval: `PRUNE_EVENT_BLOCK_INTERVAL = 84` blocks (`crates/ic-evm-gateway/src/lib.rs`)
- prune interval lower bound: `MIN_PRUNE_TIMER_INTERVAL_MS = 1_000` (for internal stored value)
- prune max ops per tick: `DEFAULT_PRUNE_MAX_OPS_PER_TICK = 5_000`
- prune min ops per tick: `MIN_PRUNE_MAX_OPS_PER_TICK = 1`
- backoff cap: `MAX_PRUNE_BACKOFF_MS = 300_000`
- operational rule: if any value above changes, sync this README in the same PR with `crates/evm-db/src/chain_data/runtime_defaults.rs` as source of truth.

## Compatibility Notes

- `eth_getStorageAt.slot` accepts both `QUANTITY` (for example `0x0`) and `DATA (32bytes)`.
- For `eth_call` / `eth_estimateGas`, `gasLimit` is accepted as `gas`.
- For `eth_call` / `eth_estimateGas`, QUANTITY accepts both hex (`0x...`) and decimal strings.
- Relationship between `rpc_eth_history_window` and `earliest`: `earliest` means block `0`; if `oldest_available > 0`, it must return `invalid.block_range.out_of_window`.
- Explicit compatibility absorptions in gateway:
  - kept: `gasLimit -> gas`, decimal-string QUANTITY normalization, normalization for `String` objects / whitespace tags
  - removed: implicit rounding of `earliest/0x...` to `latest`, hidden per-method fallback behavior
- On startup, canister API probe (`rpc_eth_history_window`) is executed; incompatibility causes fail-fast with `incompatible.canister.api`.
- `eth_getLogs` follows canister constraints (`topic1` unsupported), and gateway expands/merges `topics[0]` OR arrays. `topics[1+]` are unsupported.
- `eth_getLogs.blockHash` is resolved by scanning recent `RPC_GATEWAY_LOGS_BLOCKHASH_SCAN_LIMIT` blocks (default `2000`). Combination with `fromBlock/toBlock` is rejected as `-32602`.
- If `eth_getLogs.blockHash` cannot be resolved, returns `code=-32000`, `message="Block not found."`, `data="0x..."` (closer to EIP-234/Geth behavior).
- Input validation failures return `-32602 invalid params` (including invalid hex/length/callObject mismatch).
- `eth_call` revert returns `error.code = -32000` and hex string `error.data` (`0x...`).
- canister `Err` uses structured `RpcErrorView { code, message, error_prefix? }`.
  - gateway passes `error_prefix` through to JSON-RPC `error.data.error_prefix`
  - `1000-1999` maps to invalid params (`-32602`)
  - `2000+` maps to execution failure (`-32000`)
- `RpcErrorView.code` fixed ranges:
  - `1001`: Invalid params (length mismatch, fee/type/chainId mismatch, etc.)
  - `2001`: Execution failed (EVM execution failure)
  - `1000-1999`: reserved for invalid input
  - `2000-2999`: reserved for execution failure
- On canister side, the `wrapper` is intentionally thin and delegates RPC implementation into `ic-evm-rpc`.

## Operational Policy for `eth_getLogs` Limits (Recommended)

Before implementing production frontend logic, assume these rules:

1. Use single address + `topics[0]` (OR array up to 16 terms)
2. `blockHash` is supported, but older blocks may fail to resolve; prefer `fromBlock/toBlock`
3. Fetch logs in smaller ranges to avoid `-32005 limit exceeded`

Conditions where this is insufficient:
- need multi-contract search at once
- need `topics[1+]` filtering
- need old `blockHash` pinned search beyond `RPC_GATEWAY_LOGS_BLOCKHASH_SCAN_LIMIT`

If these gaps become blocking, implement `address[]`, OR topics, and better `blockHash` handling in that order.

## Limits (env)

- `RPC_GATEWAY_MAX_HTTP_BODY_SIZE` (default: 262144)
- `RPC_GATEWAY_MAX_BATCH_LEN` (default: 20)
- `RPC_GATEWAY_MAX_JSON_DEPTH` (default: 20)
- `RPC_GATEWAY_LOGS_BLOCKHASH_SCAN_LIMIT` (default: 2000)
- `RPC_GATEWAY_CORS_ORIGIN` (default: `*`)
  - `*` or comma-separated allowlist (for example: `https://kasane.network,http://localhost:3000`)
- `RPC_SEMANTICS_VERSION` (default: `kasane-rpc-semantics/v1`)
  - included in `web3_clientVersion`; bump when `safe/finalized` semantics change.

## Verification

```bash
npm run test
npm run lint
npm run build
npm run typecheck:worker
```

Cloudflare Workers deploy uses `wrangler.jsonc`.

```bash
npm run dev:worker
npm run deploy:staging
```

Set `RPC_GATEWAY_IDENTITY_PEM` with Cloudflare Secrets, not `vars`.
Staging uses the production canister for reads, so staging smoke must stay read-only. Production deploy is manual through the Cloudflare Deploy workflow input `deploy_production=true`.

Optional live-connect smoke:

```bash
npm run smoke:read

# monitor post-submit execution status (success when status=0x1)
npm run smoke:watch-receipt -- 0x<tx_hash> 120 1500
```

## Production Operation for receipt.status Monitoring

Minimal flow (recommended):
1. Run read-only smoke against `https://rpc.kasane.network`
2. Send one low-risk tx approved by the operator after cutover
3. Save tx hash right after `eth_sendRawTransaction`
4. Pass that hash to `smoke:watch-receipt`
5. Alert on `status!=0x1` / timeout / rpc error

Rollback: if read or write validation fails, move the Cloudflare route/DNS back to the previous RPC endpoint.

Example:

```bash
cd tools/rpc-gateway
EVM_RPC_URL="https://rpc.kasane.network" npm run smoke:read

EVM_RPC_URL="https://rpc.example.com" \
  npm run smoke:watch-receipt -- 0x<tx_hash> 180 1500
```

The systemd receipt watcher in [ops/README.md](./ops/README.md) is legacy / rollback operation. Production gateway traffic is expected to run through Cloudflare Workers.
