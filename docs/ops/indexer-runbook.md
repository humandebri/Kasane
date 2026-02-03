# Indexer Runbook v2.1（TS + SQLite + local zstd archive）

## 0. 目的と前提（壊れないための不変条件）

- Indexer（SQLite/Archive）は **キャッシュ**。チェーン本体は canister 側。
- 取り込みの整合は「**アーカイブ成功 → DBコミット → cursor更新（同一TX）**」で守る。
- canister 側 prune は **外部ACKに依存しない**（依存させると外部障害でチェーン死亡する）。

## 1. コンポーネント

- canister: `export_blocks(cursor, max_bytes)` を提供
- indexer:
  - export を poll
  - Chunk を復元して payload decode
  - SQLite に upsert
  - raw(3seg) を zstd 圧縮して `*.bundle.zst` として保存（任意キャッシュ）
  - 起動時に archive GC（tmp削除 + orphan削除）

## 1.5 dfx ローカル復旧（503/timeout対策）

前提:
- 以降の手順で **ローカル状態が消える**（問題ないことを確認）
- ログを必ず残す（`/tmp/dfx-logs` を使う）

### 1.5.1 dfx/replica/icx-proxy を物理killしてクリーン起動

```bash
scripts/dfx_local_clean_start.sh
```

別ターミナルでヘルスチェック（2秒以内に返らなければ再度 1.5.1）：

```bash
curl -m 2 -sSf http://127.0.0.1:4943/api/v2/status > /dev/null && echo "replica OK"
curl -m 2 -sSf http://127.0.0.1:8080/api/v2/status > /dev/null && echo "icx-proxy OK"
```

### 1.5.2 dfx 接続先の混入チェック

```bash
scripts/indexer_env_sanity_check.sh
```

`.env` / dotenv がある場合は同様に確認する。

### 1.5.3 canister を非対話で reinstall（InitArgs 必須）

注記:
- `null` / `opt none` / 引数省略はすべて拒否されるため、必ず `opt record` を渡す。

```bash
source scripts/lib_init_args.sh
INIT_ARGS="$(build_init_args_for_current_identity 1000000000000000000)"
dfx canister install evm_canister \
  --network local \
  --mode reinstall \
  --wasm target/wasm32-unknown-unknown/release/ic_evm_wrapper.candid.wasm \
  --argument "$INIT_ARGS" \
  2>&1 | tee /tmp/dfx-logs/deploy.log
```

ここで 503 が出たら 1.5.1 に戻る。

### 1.5.4 indexer の接続先固定チェック

```bash
rg -n "4943|8080|IC_HOST|REPLICA|icx-proxy|api/v2/status|http://127\.0\.0\.1" .
```

ハードコードがあれば、環境変数経由に逃がす（`INDEXER_IC_HOST` など）。

### 1.5.5 indexer DB/チェックポイントのクリア

まず “それっぽいディレクトリ” を探す:

```bash
find . -maxdepth 4 -type d \( -iname "*indexer*" -o -iname "*db*" -o -iname "*data*" -o -iname "*leveldb*" -o -iname "*rocksdb*" \) 2>/dev/null
```

自動化（削除は明示許可が必要）:

```bash
ALLOW_DELETE=1 scripts/indexer_reset_local_state.sh
```

次に “チェックポイントっぽいキー” を検索:

```bash
rg -n "checkpoint|cursor|last.*block|last_synced|synced_height|from_block|start_block" path/to/indexer
```

ログに DB path を出している場合は一度起動して確認:

```bash
RUST_LOG=info ./path/to/indexer 2>&1 | tee /tmp/dfx-logs/indexer_once.log
```

## 2. 起動手順

### 2.1 依存
- Node.js（`better-sqlite3` のABIが合うバージョン）
- npm install

ABIズレで落ちる場合:
- `npm rebuild better-sqlite3`

### 2.2 設定（環境変数）
主要:
- `INDEXER_DB_PATH`（SQLiteファイル）
- `INDEXER_ARCHIVE_DIR`（アーカイブ保存先）
- `INDEXER_MAX_BYTES`（export の max_bytes。推奨 1〜1.5MiB）
- `INDEXER_IDLE_POLL_MS`（追いつき時の固定ポーリング間隔。既定 1000ms）
- `INDEXER_BACKOFF_MAX_MS`（失敗時の最大バックオフ。既定 5000ms）
- `INDEXER_FETCH_ROOT_KEY`（local向け）

### 2.3 起動
- `node dist/run.js`（実際の起動コマンドはプロジェクトの package.json に合わせる）

起動直後にやること:
- archive GC が走る（失敗しても warning のみ）

## 3. 停止手順

- SIGINT / SIGTERM で停止（stop_requested を立ててループを抜ける）
- 途中停止しても、cursor は DBコミット単位でしか進まないので再開は安全

## 4. ログの見方（JSON lines）

主な event:
- `retry`: ネットワーク/呼び出し失敗（backoffあり）
- `idle`: chunks=[]（追いつき状態、60秒に1回程度）
- `fatal`: 取り込み継続不可（exit(1)）

fatal の代表:
- `Pruned`: 取り込もうとした範囲が canister 側で prune 済み
- `InvalidCursor`: cursor/chunk整合違反 or max_bytes超過 or カーソル不正
- `Decode`: payload decode 失敗
- `ArchiveIO`: アーカイブ書き込み失敗
- `Db`: SQLite 失敗

## 5. SQLite マイグレーション

- 起動時に `schema_migrations` を見て未適用SQLを適用する
- 適用は `BEGIN IMMEDIATE` で全体を1トランザクション化
- すでに適用済みの migration はスキップされる（idempotent）

運用ルール:
- migration SQL を増やしたら `MIGRATIONS` 配列に追加する

## 6. アーカイブ（zstd）

### 6.1 保存形式
- 1ブロック1ファイル: `<archiveDir>/<chainId>/<blockNumber>.bundle.zst`
- raw は 3seg を `u32be(len)+payload` で連結してから zstd 圧縮

### 6.2 atomicity
- `*.tmp` に書いて `rename`（同一FS内で原子的）
- `.tmp` が残っても起動時GCで削除される

### 6.3 起動時GC
- `.tmp` は常に削除
- orphan（DBに紐づかない `*.bundle.zst`）は **DBに参照が1件以上ある場合のみ削除**
  - DBが空の状態で「全削除」しないための安全弁

## 7. 日次メトリクス（metrics_daily）

最低限の観測:
- `blocks_ingested`（コミット1回につき +1）
- `raw_bytes`（取り込んだ raw ）
- `compressed_bytes`（zstd後）
- `sqlite_bytes`（現状は「SQLiteファイルサイズ（bytes）」を日次で保存。差分は集計側で計算）
- `archive_bytes`（現状は「アーカイブディレクトリ総サイズ（bytes）」を日次で保存）

注:
- サイズ計測は「その日の最初のコミット時」に更新（best-effort）

## 8. 典型障害と復旧

### 8.1 Pruned で停止した
意味:
- indexer が追いつく前に canister が古いブロックを prune した
- その範囲は canister からはもう取れない

対応:
1) まず canister 側の `pruned_before_block` を確認
2) indexer の cursor を `pruned_before_block + 1` 以降に進めて再開
3) 過去分が必要なら「アーカイブが残っている範囲」から再構築（アーカイブが無いなら復旧不能）

再発防止:
- pruning を有効化する前に indexer を常時稼働させ、lag を監視する
- hard_emergency が発動する前に通常水位で prune できるようにする

### 8.2 InvalidCursor / Decode
- 仕様違反か実装バグの可能性が高い
- `fatal` ログに `cursor / next_cursor / chunks_summary` が出るので、その組み合わせで再現テストを作る

### 8.3 ArchiveIO
- ディスク枯渇・権限・別FSへのrenameなど
- まず保存先の空き容量/パーミッション確認

## 9. pruning の段階的ON（canister側）

推奨フロー（事故りにくい順）:
1) **policy だけ投入**（enabled=false のまま）
2) `get_prune_status` で水位・oldest・推定容量を確認
3) enabled=true にして timer を動かす（最初は小さく）

初期推奨パラメータ（例）:
- `timer_interval_ms`: 30_000〜60_000
- `max_ops_per_tick`: 200〜500（最初は小さく）
- `headroom_ratio_bps`: 2000（20%）
- `hard_emergency_ratio_bps`: 9500（95%）
- `retain_days`: 14（監査重視なら 30）
- `target_bytes`: 実測（bytes/day）× retain_days × (1+headroom) で決める

次（実装の続きとしてやるべき順）

ドキュメントじゃなくて実装の話に戻すと、もう **「実測→policy決定→段階的ON」**のフェーズだから、次の3つだけやればいい。

canister 側の get_prune_status を indexer 側に定期pullして meta に書く（head/pruned_before/estimated_kept_bytes/stable_pages）

cursor_lag（head - cursor）をメトリクス化（日次じゃなくてもいい、ログでもいい）

pruning enable の手順をスクリプト化（set_policy → enabled=true をワンコマンド化）
### 7.1 prune_status 監視

* `get_prune_status()` を定期ポーリングして `meta.prune_status` に JSON 保存
* JSON は `estimated_kept_bytes` / `high_water_bytes` / `hard_emergency_bytes` を文字列で保持して追跡
* 監視側は `need_prune` フラグと `cursor_lag` を合わせてアラート

## 10. ローカル統合スモーク（最優先）

狙い: 「設計は正しいが、実接続で死ぬ」事故を潰す。

前提:
- `dfx`, `cargo`, `npm`, `python` が使える
- 既存のlocal dfxを止めて良い（`DFX_CLEAN=1` が既定）

手順:
1) ローカルIC起動 + canisterデプロイ + tx投入 + indexer起動 + 検証を **一括** 実行

```bash
scripts/local_indexer_smoke.sh
```

確認されること:
- pruning は `enabled=false` のまま
- tx投入 → block生成
- indexer起動 → cursor前進 / archive生成 / metrics_daily埋まる
- 追いついたら idle（1秒ポーリング + 60秒に1回の idle ログ）

失敗時は `INDEXER_LOG` を確認すること。

## 11. 失敗注入（運用で死ぬところを先に殺す）

狙い: 夜間運用で起こりやすい復旧/再起動パスを先に通す。

```bash
scripts/local_indexer_fault_injection.sh
```

実施内容:
- ingest中に indexer を kill → 同じcursorから復旧（DBトランザクション境界の確認）
- `.tmp` を残した状態で再起動 → 起動時GCで削除される
- canister停止（dfx stop）→ retry/backoff が暴走せずログが読みやすい

## 12. pruning は段階的に実地確認（いきなりONしない）

狙い: pruning有効化の事故を防ぐ。

```bash
scripts/local_pruning_stage.sh
```

確認されること:
- `need_prune` が enabled=false でも true になり得る
- ゆるい policy → export が `Pruned` を返さない範囲で prune が進む
- aggressive policy → `Pruned` を発生させ、止まり方と復旧手順を確認

## 13. 24h 実測（容量の意思決定は最後に数字で殴る）

狙い: 1日あたり増加量/圧縮率/prune policyの実値を出す。

運用:
- indexer を **24h連続稼働** させる
- `metrics_daily` を毎日確認する

補助:
```bash
DB_PATH=tools/indexer/indexer.db scripts/indexer_metrics_snapshot.sh
```
