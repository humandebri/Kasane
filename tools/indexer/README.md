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
- `INDEXER_MAX_SEGMENT` (任意, 既定: 2, `next_cursor.segment` の許容上限)

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
- segment は **0..INDEXER_MAX_SEGMENT**（既定は 0/1/2）
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

## ops_metrics_samples

- canister `metrics(128)` を `INDEXER_OPS_METRICS_POLL_MS` 間隔で保存します。
- 保存項目: `queue_len / total_submitted / total_included / total_dropped / drop_counts_json`
- 保存時に 14日より古いサンプルを削除します（retention固定）。

## tx_index payload 仕様（固定）

segment `2` の各エントリは以下の固定順序:

`[tx_hash:32][entry_len:4][block_number:8][tx_index:4][caller_principal_len:2][caller_principal:caller_principal_len][from:20][to_len:1][to:to_len]`

- すべて Big Endian
- `entry_len = 12 + 2 + caller_principal_len + 20 + 1 + to_len`
- `to_len` は `0`（contract creation）または `20` のみ許可
- 後方互換モードは持たない（旧形式は不正として reject）
- `caller_principal_len=0` の場合は principal なしとして扱う

## マイグレーション（Postgres）

起動時に `schema_migrations` を見て **未適用のSQLのみ** 実行する。

```
tools/indexer/migrations/
  001_init.sql
  002_backfill.sql
  003_add_txs_caller_principal_index.sql
  004_add_txs_from_to_addresses.sql
  005_add_receipt_status_and_ops_metrics.sql
```
