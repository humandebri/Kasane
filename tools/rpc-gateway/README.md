# RPC Gateway (Phase2)

Gateway前提で canister Candid API を Ethereum風 JSON-RPC 2.0 に変換する実装です。

## セットアップ

```bash
cd tools/rpc-gateway
npm install
cp .env.example .env.local
```

`.env.local` で最低限 `EVM_CANISTER_ID` を設定してください。

## 起動

```bash
npm run dev
```

既定: `http://127.0.0.1:8545`

## 対応メソッド

- `web3_clientVersion`
- `net_version`
- `eth_chainId`
- `eth_blockNumber`
- `eth_syncing`
- `eth_getBlockByNumber`
- `eth_getTransactionByHash`
- `eth_getTransactionReceipt`
- `eth_getBalance` (`latest` のみ)
- `eth_getCode` (`latest` のみ)
- `eth_getStorageAt` (`latest` のみ)
- `eth_call(callObject, blockTag)` (`latest` のみ)
- `eth_estimateGas(callObject, blockTag)` (`latest` のみ)
- `eth_sendRawTransaction`

## callObject 対応範囲（Phase2.2）

- サポート: `to`, `from`, `gas`, `gasPrice`, `value`, `data`, `nonce`, `maxFeePerGas`, `maxPriorityFeePerGas`, `chainId`, `type`, `accessList`
- `type` は `0x0` / `0x2` のみ受理
- `accessList` は EIP-2930 形式（`address`, `storageKeys[]`）を受理
- `nonce` 省略時は canister 側で `from` アカウントの現在 nonce を既定利用
- 未対応フィールドは `-32602 invalid params`
- バリデーション:
  - `gasPrice` と `maxFeePerGas` / `maxPriorityFeePerGas` の併用は禁止
  - `maxPriorityFeePerGas` 指定時は `maxFeePerGas` 必須
  - `maxPriorityFeePerGas <= maxFeePerGas`
  - `type=0` と `max*` は併用禁止
  - `type=2` と `gasPrice` は併用禁止

## Ethereum JSON-RPC互換詳細

以下は**現行実装時点**の互換詳細です。本セクションを互換表の更新正本とし、変更時は root README の要約表も同一PRで同期更新します。

| Method | Status | Current behavior | Limitation | Alternative/Note |
| --- | --- | --- | --- | --- |
| `eth_chainId` | Supported | canister の `rpc_eth_chain_id` を返す | なし | `net_version` は10進文字列で同値を返す |
| `eth_blockNumber` | Supported | canister の `rpc_eth_block_number` を返す | なし | - |
| `eth_syncing` | Supported | 常に `false` を返す | 同期進捗オブジェクト非対応 | 即時実行モデル前提 |
| `eth_getBlockByNumber` | Supported | `blockTag` を解決してブロックを返す | `latest/pending/safe/finalized` は head 扱い | canister では `rpc_eth_get_block_by_number` |
| `eth_getTransactionByHash` | Supported | `eth_tx_hash` で取引を参照する | `tx_id` 直接参照なし。migration未完了/critical corrupt時は `-32000 state unavailable` | canister では `rpc_eth_get_transaction_by_eth_hash` |
| `eth_getTransactionReceipt` | Supported | `eth_tx_hash` で receipt を参照する | `tx_id` 直接参照なし。migration未完了/critical corrupt時は `-32000 state unavailable` | canister では `rpc_eth_get_transaction_receipt_by_eth_hash` |
| `eth_getBalance` | Partially supported | 残高取得を返す | `blockTag` は `latest` 系のみ | 不正入力は `-32602` |
| `eth_getCode` | Partially supported | コードを返す | `blockTag` は `latest` 系のみ | 不正入力は `-32602` |
| `eth_getStorageAt` | Partially supported | ストレージ値を返す | `blockTag` は `latest` 系のみ | `slot` は QUANTITY/DATA(32bytes) の両対応 |
| `eth_call` | Partially supported | callObject を canister に委譲 | `blockTag` は `latest` 系のみ、未対応フィールド拒否 | revert は `-32000` + `error.data` |
| `eth_estimateGas` | Partially supported | callObject を使って見積り | `blockTag` は `latest` 系のみ、未対応フィールド拒否 | canister `Err` を `-32602`/`-32000` にマップ |
| `eth_sendRawTransaction` | Supported | 生txを canister submit API に委譲し、返却 `tx_id` から `eth_tx_hash` を解決して `0x...` を返す | submit失敗はJSON-RPCエラーへマップ。`eth_tx_hash` 解決不能時は `-32000` を返す | canister では `rpc_eth_send_raw_transaction` |
| `eth_getLogs` | Not supported | `handlers.ts` にJSON-RPCハンドラがないため `method not found` を返す | 標準 filter API は未提供 | canister 側代替は `rpc_eth_get_logs_paged` |
| `eth_newFilter` / `eth_getFilterChanges` / `eth_uninstallFilter` | Not supported | filter系は未実装 | Phase2スコープ外 | `rpc_eth_get_logs_paged` を利用 |
| `eth_subscribe` / `eth_unsubscribe` | Not supported | WebSocket購読は未実装 | Phase2スコープ外 | `eth_blockNumber` ポーリング運用 |
| pending / mempool 系（例: `eth_pendingTransactions`） | Not supported | pending/mempool概念を提供しない | Phase2スコープ外 | submit後にブロック生成と参照RPCで追跡 |

本ドキュメントの互換表は JSON-RPC 層を対象とし、opcode 実行意味論の差分整理は現時点の対象外です。

従来のEVMチェーンと異なる運用上の注意（現行実装時点）:
- Pruning: canister は履歴を prune するため、古い範囲は `rpc_eth_get_block_by_number_with_status` / `rpc_eth_get_transaction_receipt_with_status` で `Pruned` / `PossiblyPruned` が返り得ます。
- Timer駆動: canister 側で timer により mining/pruning を実行します。mining は `set_timer` の単発予約を毎tickで再設定する方式で、`mining_scheduled` フラグにより多重予約を防ぎます。
- Timer駆動（mining詳細）: `set_auto_mine(false)` 運用では `produce_block` を明示的に呼ぶ必要があります。`ready_queue` が空のときは空ブロックを作らず、次回予約のみ行います。
- Timer駆動（backoff/停止条件）: `produce_block` 失敗時は指数バックオフ（2倍、上限 `MAX_MINING_BACKOFF_MS`）を適用し、成功で基本間隔へ戻します。cycle critical または migration 中は write 拒否により採掘を停止し、復帰後は cycle observer tick（60s）が再スケジュールを補助します。
- Submit/Execute分離: `eth_sendRawTransaction` は投入APIへの委譲で、実行確定は別フェーズ（block production）です。
- `eth_sendRawTransaction` 戻り値: Gateway は canister `rpc_eth_send_raw_transaction` の返却 `tx_id` から `rpc_eth_get_transaction_by_tx_id` で `eth_tx_hash` を解決して返します。解決不能時は `-32000` エラーを返します。
- `eth_getTransactionReceipt.logs[].logIndex`: ブロック内通番で返します。
- Hash semantics: canister内部では `tx_id` を保持し、Ethereum互換参照は `eth_tx_hash` を使用します。Gateway は `eth_*ByHash` を `eth_tx_hash` 系に接続します。
- Finality assumptions: 単一シーケンサ前提で reorg 前提の挙動は提供しません。

関連定数（現行実装値）:
- mining 基本間隔: `DEFAULT_MINING_INTERVAL_MS = 5_000`
- cycle observer 間隔: `60s`（`set_timer_interval(Duration::from_secs(60), ...)`）
- prune 基本間隔: `DEFAULT_PRUNE_TIMER_INTERVAL_MS = 3_600_000`（1時間）
- prune 間隔の下限: `MIN_PRUNE_TIMER_INTERVAL_MS = 1_000`
- prune 1tick上限: `DEFAULT_PRUNE_MAX_OPS_PER_TICK = 5_000`
- prune 1tick最小: `MIN_PRUNE_MAX_OPS_PER_TICK = 1`
- backoff 上限: `MAX_MINING_BACKOFF_MS = 300_000`, `MAX_PRUNE_BACKOFF_MS = 300_000`
- 運用ルール: 上記の実値を変更する場合は `crates/evm-db/src/chain_data/runtime_defaults.rs` を正本として同一PRで本READMEを同期更新すること。

## 互換ノート

- `eth_getStorageAt` の `slot` は `QUANTITY`（例: `0x0`）と `DATA(32bytes)` の両方を受理します。
- 入力不正は `-32602 invalid params` を返します（hex不正/長さ不正/callObject不整合を含む）。
- `eth_call` の revert は `error.code = -32000` で、`error.data` に hex 文字列（`0x...`）を返します。
- canister `Err` は `RpcErrorView { code, message }` の構造化形式です。
  - `1000-1999` は入力不正として `-32602`
  - `2000+` は実行失敗として `-32000`
- `RpcErrorView.code` 固定値（Phase2.2）:
  - `1001`: Invalid params（長さ不正、fee/type/chainId不整合など）
  - `2001`: Execution failed（EVM実行失敗）
  - `1000-1999`: 入力不正予約帯
  - `2000-2999`: 実行失敗予約帯
- canister 側は分離方針に合わせて `wrapper` を薄い委譲層にし、RPC実装は `ic-evm-rpc` 側に集約しています。

## 制限値（env）

- `RPC_GATEWAY_MAX_HTTP_BODY_SIZE` (default: 262144)
- `RPC_GATEWAY_MAX_BATCH_LEN` (default: 20)
- `RPC_GATEWAY_MAX_JSON_DEPTH` (default: 20)

## 検証

```bash
npm run test
npm run lint
npm run build
```

実接続スモーク（任意）:

```bash
npm run smoke:all
```
