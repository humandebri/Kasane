# Kasane

![license](https://img.shields.io/badge/license-Apache--2.0-blue)
[![canister](https://img.shields.io/badge/canister-4c52m--aiaaa--aaaam--agwwa--cai-1f6feb)](https://dashboard.internetcomputer.org/canister/4c52m-aiaaa-aaaam-agwwa-cai)
[![network](https://img.shields.io/badge/network-ic-2ea44f)](https://dashboard.internetcomputer.org/)
![chain_id](https://img.shields.io/badge/chain--id-4801360-f59e0b)

Kasane is an EVM-compatible execution environment implemented as Internet Computer canisters. It exposes Candid APIs for IC-native callers and a JSON-RPC gateway for Ethereum-style tooling.

## Network

| Item | Value |
| --- | --- |
| Environment | testnet |
| Network name | `kasane` |
| Canister ID | `4c52m-aiaaa-aaaam-agwwa-cai` |
| Chain ID | `4801360` |
| RPC URL | `https://rpc-testnet.kasane.network` |
| Explorer URL | `https://explorer-testnet.kasane.network` |

## Runtime Model

- `submit_ic_tx` and `rpc_eth_send_raw_transaction` enqueue transactions; block production finalizes execution later.
- Submission success is not execution success. Check `eth_getTransactionReceipt.status` (`0x1` success, `0x0` execution failure).
- `tx_id` is the internal canister identifier. `eth_tx_hash` is the Ethereum-compatible `keccak256(raw_tx)` hash.
- Nonce lookup uses `eth_getTransactionCount` through the gateway or `expected_nonce_by_address` through canister query APIs.
- The native display asset is `ICP`, represented with the EVM convention of `10^18` base units.

## APIs

### `submit_ic_tx`

`submit_ic_tx(record)` accepts IC-originated synthetic EVM transactions.

| Field | Type | Notes |
| --- | --- | --- |
| `to` | `opt vec nat8` | `20` bytes for a call target, `null` for contract creation |
| `value` | `nat` | uint256 value |
| `gas_limit` | `nat64` | gas limit |
| `nonce` | `nat64` | sender nonce |
| `max_fee_per_gas` | `nat` | EIP-1559 max fee, bounded by u128 |
| `max_priority_fee_per_gas` | `nat` | EIP-1559 priority fee, bounded by u128 |
| `data` | `vec nat8` | calldata |

The sender is not part of the payload. The canister derives a 20-byte EVM sender address from `msg_caller()` using the Chain Fusion signer derivation compatible with `@dfinity/ic-pub-key`.

### `rpc_eth_send_raw_transaction`

`rpc_eth_send_raw_transaction(raw_tx: blob)` accepts signed Ethereum raw transactions.

- Supported formats: Legacy RLP, EIP-2930, EIP-1559.
- Unsupported formats: EIP-4844 (`0x03`), EIP-7702 (`0x04`).
- Return value: internal `tx_id` (`32` bytes).

## JSON-RPC Gateway

The gateway lives in [tools/rpc-gateway/README.md](tools/rpc-gateway/README.md). Supported methods include:

- `eth_chainId`
- `eth_blockNumber`
- `eth_gasPrice`
- `eth_maxPriorityFeePerGas`
- `eth_feeHistory`
- `eth_getBlockByNumber`
- `eth_getTransactionByHash`
- `eth_getTransactionReceipt`
- `eth_getBalance`
- `eth_getTransactionCount`
- `eth_getCode`
- `eth_getStorageAt`
- `eth_getLogs`
- `eth_call`
- `eth_estimateGas`
- `eth_sendRawTransaction`

Filter APIs, subscriptions, mempool APIs, and block-hash indexed block lookups are not implemented.

## Precompiles

`0x00000000000000000000000000000000ffff0003` is reserved for the ICP query precompile.

- The ABI is a compact binary payload: `version`, `kind`, `target_principal`, `method`, and raw Candid argument bytes.
- v1 is query-only. `kind=1` update calls are rejected because EVM transaction revert semantics do not make external IC side effects atomic.
- Allowed `(target, method)` pairs are controller-managed and must refer to query or composite query methods.
- The composite query entrypoint calls allowlisted methods with bounded wait and a 1 second timeout, returning raw Candid reply bytes.
- Each `eth_call` may invoke the ICP query precompile at most once. A second call reverts with `ic_query.call_limit`.
- Two-pass execution compares the initial and post-query snapshots, including chain state, runtime config, allowlist fingerprint, and `evm_state_epoch`.

`0x00000000000000000000000000000000ffff0004` is reserved for ICP update intents.

- The ABI uses the same compact payload shape with `kind=1`.
- Execution records an allowlisted update intent log; the remote IC update call is dispatched after block production.
- Allowed `(target, method)` pairs are controller-managed with `add_update_precompile_allowed_method` and `remove_update_precompile_allowed_method`.

The default build disables precompiles that require unsupported or intentionally excluded upstream feature sets:

- EIP-4844 `KZG_POINT_EVALUATION` at `0x0a`.
- Prague/Osaka BLS12-381 precompiles at `0x0b` through `0x11`.
- Osaka `P256VERIFY` at `0x0100`.

The default precompile extra-gas ratio is fixed in code at `1/100`:

```text
extra_gas = ceil(elapsed_instruction / 100)
```

Measurement-only APIs are guarded by the `precompile-profile-admin` feature.

## Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/` | Rust canisters, EVM core, database, RPC types, metrics, and verification crates |
| `docs/api/` | API usage guides |
| `docs/specs/` | implementation specs |
| `docs/verification/` | verification boundary and TCB documents |
| `scripts/` | local checks, smoke tests, deployment helpers, and maintenance scripts |
| `tools/rpc-gateway/` | Ethereum JSON-RPC gateway |
| `tools/indexer/` | Postgres-backed pull indexer |
| `tools/explorer/` | Next.js explorer |
| `tools/wrapper-vite/` | wrapper frontend |

## Development

Run the standard local check first:

```bash
cargo check --workspace
```

Common verification commands:

```bash
CI_LOCAL_MODE=github scripts/ci-local.sh
scripts/predeploy_smoke.sh
scripts/query_smoke.sh
```

Query calls to canisters should use `dfx canister call --query ...`.

## Documentation

- Frontend IC wallet flow: [docs/api/frontend_submit_ic_tx_guide.md](docs/api/frontend_submit_ic_tx_guide.md)
- Raw transaction payload flow: [docs/api/rpc_eth_send_raw_transaction_payload.md](docs/api/rpc_eth_send_raw_transaction_payload.md)
- Transaction hash semantics: [docs/data_tx_id.md](docs/data_tx_id.md)
- Indexer export spec: [docs/specs/indexer-v1.md](docs/specs/indexer-v1.md)
- Verification architecture: [docs/verification/README.md](docs/verification/README.md)
- Script guide: [scripts/README.md](scripts/README.md)

## License

Apache-2.0. See [LICENSE](LICENSE).
