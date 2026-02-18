# Config Reference（自動生成）

## TL;DR
- 必須設定は Gateway: `EVM_CANISTER_ID`、Indexer: `EVM_CANISTER_ID` + `INDEXER_DATABASE_URL`。
- 既定の `chain_id` は `4801360`。
- ガス/採掘/prune の既定値は Rust定数で管理される。

## 1. Gateway環境変数
定義: `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/config.ts`

- 必須
  - `EVM_CANISTER_ID`
- 任意（既定値）
  - `RPC_GATEWAY_IC_HOST`（`https://icp-api.io`）
  - `RPC_GATEWAY_FETCH_ROOT_KEY`（`false`）
  - `RPC_GATEWAY_IDENTITY_PEM_PATH`（`null`）
  - `RPC_GATEWAY_HOST`（`127.0.0.1`）
  - `RPC_GATEWAY_PORT`（`8545`）
  - `RPC_GATEWAY_CLIENT_VERSION`（`kasane/phase2-gateway/v0.1.0`）
  - `RPC_GATEWAY_MAX_HTTP_BODY_SIZE`（`262144`）
  - `RPC_GATEWAY_MAX_BATCH_LEN`（`20`）
  - `RPC_GATEWAY_MAX_JSON_DEPTH`（`20`）
  - `RPC_GATEWAY_CORS_ORIGIN`（`*`）

## 2. Indexer環境変数
定義: `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/src/config.ts`

- 必須
  - `EVM_CANISTER_ID`
  - `INDEXER_DATABASE_URL`
- 任意（既定値）
  - `INDEXER_IC_HOST`（`https://icp-api.io`）
  - `INDEXER_DB_POOL_MAX`（`10`）
  - `INDEXER_RETENTION_DAYS`（`90`）
  - `INDEXER_RETENTION_ENABLED`（`true`）
  - `INDEXER_RETENTION_DRY_RUN`（`false`）
  - `INDEXER_ARCHIVE_GC_DELETE_ORPHANS`（`false`）
  - `INDEXER_MAX_BYTES`（`1200000`）
  - `INDEXER_BACKOFF_INITIAL_MS`（`200`）
  - `INDEXER_BACKOFF_MAX_MS`（`5000`）
  - `INDEXER_IDLE_POLL_MS`（`1000`）
  - `INDEXER_PRUNE_STATUS_POLL_MS`（`30000`）
  - `INDEXER_OPS_METRICS_POLL_MS`（`30000`）
  - `INDEXER_FETCH_ROOT_KEY`（`false`）
  - `INDEXER_ARCHIVE_DIR`（`./archive`）
  - `INDEXER_CHAIN_ID`（`4801360`）
  - `INDEXER_ZSTD_LEVEL`（`3`）
  - `INDEXER_MAX_SEGMENT`（`2`）

## 3. Chain / Runtime定数

### chain constants
定義: `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-db/src/chain_data/constants.rs`
- `CHAIN_ID = 4_801_360`
- `MAX_TX_SIZE = 128 * 1024`
- `MAX_TXS_PER_BLOCK = 1024`
- `MAX_PENDING_GLOBAL = 8192`
- `MAX_PENDING_PER_SENDER = 64`
- `MAX_PENDING_PER_PRINCIPAL = 32`
- `MAX_NONCE_WINDOW = 64`
- `MAX_LOGS_PER_TX = 64`
- `MAX_LOG_TOPICS = 4`

### runtime defaults
定義: `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-db/src/chain_data/runtime_defaults.rs`
- `DEFAULT_MINING_INTERVAL_MS = 2000`
- `DEFAULT_BASE_FEE = 250_000_000_000`
- `DEFAULT_MIN_GAS_PRICE = 250_000_000_000`
- `DEFAULT_MIN_PRIORITY_FEE = 250_000_000_000`
- `DEFAULT_BLOCK_GAS_LIMIT = 3_000_000`
- `DEFAULT_INSTRUCTION_SOFT_LIMIT = 4_000_000_000`
- `DEFAULT_PRUNE_TIMER_INTERVAL_MS = 3_600_000`
- `DEFAULT_PRUNE_MAX_OPS_PER_TICK = 5_000`
- `MIN_PRUNE_TIMER_INTERVAL_MS = 1_000`
- `MIN_PRUNE_MAX_OPS_PER_TICK = 1`

## 4. 危険値・注意
- `INDEXER_CHAIN_ID` が実チェーンと不一致だと署名/検証系の運用が破綻しうる。
- `RPC_GATEWAY_MAX_HTTP_BODY_SIZE` を過大化すると DoS耐性が下がる。
- `DEFAULT_MIN_GAS_PRICE` / `DEFAULT_MIN_PRIORITY_FEE` を無根拠で下げると低feeスパム耐性が下がる。

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/config.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/indexer/src/config.ts`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-db/src/chain_data/constants.rs`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-db/src/chain_data/runtime_defaults.rs`
