# Indexer Worker (Postgres-first)

目的: canister の export API を pull して Postgres に最小インデックスを作る。

## 使い方

```bash
cd tools/indexer
npm install
npm run dev
```

## 環境変数

- `.env.local` はローカル用のテンプレート
- `.env.example` は配布用のひな型

- `EVM_CANISTER_ID` (必須)
- `INDEXER_DATABASE_URL` (必須, 例: `postgres://postgres:postgres@127.0.0.1:5432/kasane`)
- `INDEXER_DB_POOL_MAX` (任意, 既定: 10)
- `INDEXER_RETENTION_ENABLED` (任意, 既定: true)
- `INDEXER_RETENTION_DAYS` (任意, 既定: 90)
- `INDEXER_RETENTION_DRY_RUN` (任意, 既定: false)
- `INDEXER_ARCHIVE_GC_DELETE_ORPHANS` (任意, 既定: false)
- `INDEXER_IC_HOST` (任意, 既定: https://icp-api.io)
- `INDEXER_MAX_BYTES` (任意, 既定: 1200000)
- `INDEXER_BACKOFF_INITIAL_MS` (任意, 既定: 200)
- `INDEXER_BACKOFF_MAX_MS` (任意, 既定: 5000)
- `INDEXER_IDLE_POLL_MS` (任意, 既定: 1000)
- `INDEXER_PRUNE_STATUS_POLL_MS` (任意, 既定: 30000)
- `INDEXER_OPS_METRICS_POLL_MS` (任意, 既定: 30000)
- `INDEXER_FETCH_ROOT_KEY` (任意, 1/true で有効。local向け)
- `INDEXER_ARCHIVE_DIR` (任意, 既定: ./archive)
- `INDEXER_CHAIN_ID` (任意, 既定: 4801360)
- `INDEXER_ZSTD_LEVEL` (任意, 既定: 3)
- `INDEXER_MAX_SEGMENT` (任意, 既定: 3, `next_cursor.segment` の許容上限)

注: ローカル（dfx）向けに接続する場合は `INDEXER_IC_HOST` を `http://127.0.0.1:4943` にし、`INDEXER_FETCH_ROOT_KEY=true` を推奨。

## Cursor JSON（固定）

```
{
  "v": 1,
  "block_number": "u64",
  "segment": 0,
  "byte_offset": 0
}
```

- block_number は **10進ASCII、先頭0なし**（"0"は許可）
- segment は **0..INDEXER_MAX_SEGMENT**（既定は 0/1/2/3）
- byte_offset は **0..=u32**

運用メモ:
- DBに保存された cursor の `segment` が `INDEXER_MAX_SEGMENT` を超えている場合、起動時に停止する。
- canister 側で segment 定義を拡張した場合、デプロイ時に `INDEXER_MAX_SEGMENT` も同値へ更新する。
- `export_blocks` が `Err.Pruned` を返した場合、indexer は `pruned_before_block + 1`（最小1、最大head）へ自動補正して同期を継続する。

## idle / retry（運用）

- 追いつき時は `INDEXER_IDLE_POLL_MS` で **固定間隔**ポーリング
- **指数バックオフはネットワーク失敗時のみ**

## archive_parts

`archive_parts` は **再構築可能なキャッシュ**。消しても canister から再作成できる。
起動時に **DBに紐づかないアーカイブファイルは削除** される可能性がある。

## metrics_daily

`archive_bytes` は日次の実サイズ（差分は集計側で算出）。

## txs / receipt_status

- `txs.receipt_status` に `segment=1` のreceipt payloadから抽出した `0|1` を保存します。
- payload不正時は fatal で停止します（整合性優先）。

## internal_transactions

- `segment=3` の internal trace payloadから `internal_transactions` を保存します。
- 保存項目: `tx_hash / block_number / tx_index / trace_id / trace_sort_key / depth / action_type / from_address / to_address / created_contract_address / value_numeric / success / error_code`
- address画面では `from_address / to_address / created_contract_address` のいずれか一致で参照します。
- `trace_sort_key` は `trace_id` の数値順を安定化するための内部列です。
- `delegatecall` / `staticcall` も保存対象ですが、explorer UI v1 では非表示です。

## token_transfers (ERC-20)

- `segment=1` のreceipt payloadから `Transfer(address,address,uint256)` ログを抽出し、`token_transfers` に保存します。
- 保存項目: `tx_hash / block_number / tx_index / log_index / token_address / from_address / to_address / amount_numeric`
- `topic0` が Transfer でも `topic1/topic2/data(>=32byte)` が不正なログは、そのログだけをスキップして取り込み継続します。
- `amount` は ABI準拠で先頭32byteのみを使用します。`numeric(78,0)` に収まらない値や行単位INSERT失敗は、その行だけスキップして取り込み継続します。
- `commit_block` ログに `token_transfer_skipped_*` を出力し、スキップ件数を監視できます。

## ops_metrics_samples

- canister `metrics(128)` を `INDEXER_OPS_METRICS_POLL_MS` 間隔で保存します。
- 保存項目: `queue_len / cycles / total_submitted / total_included / total_dropped / drop_counts_json`
- 保存時に 14日より古いサンプルを削除します（retention固定）。

## tx_index payload 仕様（固定）

segment `2` の各エントリは以下の固定順序:

`[tx_hash:32][entry_len:4][block_number:8][tx_index:4][caller_principal_len:2][caller_principal:caller_principal_len][from:20][to_len:1][to:to_len][selector_len:1][selector:selector_len][eth_hash_len:1][eth_hash:eth_hash_len]`

- すべて Big Endian
- `entry_len = 12 + 2 + caller_principal_len + 20 + 1 + to_len + 1 + selector_len + 1 + eth_hash_len`
- `to_len` は `0`（contract creation）または `20` のみ許可
- `selector_len` は `0` または `4` のみ許可（必須）
- `eth_hash_len` は `0` または `32` のみ許可（必須）
- 旧形式（selector_len未付与）は reject
- `caller_principal_len=0` の場合は principal なしとして扱う

## internal trace payload 仕様（v2）

segment `3` の各エントリは以下の固定順序:

`[tx_hash:32][entry_len:4][version:1][truncated:1][captured_count:4][total_count:4][trace_count:4][trace...]`

各 `trace` は以下:

`[block_number:8][tx_index:4][trace_id_len:2][trace_id:trace_id_len][depth:2][action_type:1][from:20][to_len:1][to:to_len][value:32][created_len:1][created:created_len][success:1][error_len:2][error:error_len]`

- すべて Big Endian
- `version=2` のみ受理
- `captured_count` は `trace_count` と一致する必要がある
- `total_count > captured_count` のとき、その tx の internal trace は canister 側で上限打ち切り済み
- `to_len` と `created_len` は `0` または `20` のみ許可
- `trace_id` は flatten 済みの親子パス（例: `0`, `0_1`, `0_1_0`）
- `action_type` は `call / callcode / delegatecall / staticcall / create / create2 / custom / selfdestruct` を表す固定 enum
- `value` は unsigned 256-bit integer として decode し、`numeric(78,0)` に保存する

## 既存 `eth_tx_hash` 欠損の手動補完

運用で一度だけ実行する補完CLI:

```bash
cd tools/indexer
npm run backfill:eth-hash
```

任意パラメータ:
- `INDEXER_BACKFILL_BATCH_SIZE` (既定: 200)
- `INDEXER_BACKFILL_MAX_BATCHES` (既定: 0=無制限)
- `INDEXER_BACKFILL_RETRY_MAX` (既定: 2)
- `INDEXER_BACKFILL_RETRY_SLEEP_MS` (既定: 100)

## マイグレーション（Postgres）

起動時に `schema_migrations` を見て **未適用のSQLのみ** 実行する。

```
tools/indexer/migrations/
  001_init.sql
  002_backfill.sql
  003_add_txs_caller_principal_index.sql
  004_add_txs_from_to_addresses.sql
  005_add_receipt_status_and_ops_metrics.sql
  006_add_token_transfers.sql
  007_add_ops_metrics_cycles.sql
  008_add_blocks_gas_used.sql
  009_add_txs_selector.sql
  021_add_internal_transactions.sql
  022_add_internal_transaction_sort_key.sql
```
