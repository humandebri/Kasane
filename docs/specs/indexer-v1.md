# Indexer 実装Spec v1（pull + Supabase）

## 結論：おすすめ構成（現実に回るやつ）

1) 取り込みは **外部ワーカー** が pull  
2) canister の `export_blocks(cursor, max_bytes)` を **定期ポーリング**  
3) 取得分を Supabase Postgres に **直結で INSERT/UPSERT**  
4) 進捗 cursor は **DB に保存**（落ちても復帰可能）

Edge Functions だけでも可能だが、EVM logs が増えると **CPU/メモリが先に詰まる**。  
**常駐ワーカー（Rust/TS）**が最も安定。

**方針A（外部DBはキャッシュ）**:
* 外部DBは **派生データ**として扱い、チェーンの正しさ/進行に影響させない  
* prune は **target_bytes / retain_days のみ**で決定する  
* indexer停止中に prune が進む可能性はある（観測性の損失として許容）

---

## 1) 取り込みフロー（pull）

* `get_head()` で head を確認  
* `export_blocks(cursor, max_bytes)` を **cursor 기반**で繰り返し取得  
* 取得した `BlockBundle` を **同一ブロック単位**で DB に保存  
* `cursor` は DB に保存（再起動で継続可能）

推奨 `max_bytes`:
* 1,000,000〜1,500,000 bytes  
* Candid オーバーヘッドを考慮して **余裕を持たせる**

---

## 2) Supabase で効く機能（厳選）

### A) パーティショニング（必須）

**logs / transactions / receipts は肥大化が速い。**  
最初から partition 前提で設計すること。

* logs: `block_number` の range partition  
  * 例: 100k blocks 単位
* transactions / receipts: `block_number` で同様に partition  
  * JOIN が楽になる

「あとで移行」は地獄。**最初から切る。**

### B) キューが必要なら pgmq

* fetch → parse → write を分離したい場合に有効  
* Redis 不要で **DB 内耐久キュー**が作れる  
* ブロック番号レンジをメッセージ化して **水平スケール**可能

### C) 定期実行は pg_cron

* パーティション作成  
* 古い raw の削除  
* 軽いメンテ用途

Edge Functions の定期実行は **pg_cron + pg_net** で代替できるが、  
**重い処理は外部ワーカー推奨**。

---

## 3) Postgres 17 以降の注意

* 拡張が削除/非推奨になる可能性がある  
* 代表例: **timescaledb**

**timescaledb 前提は避ける。**  
素の Postgres partitioning（range/list）で組むのが安全。

---

## 4) 最小スキーマ（推奨）

```
blocks(
  number PK,
  hash,
  parent_hash,
  ts,
  tx_count
)

transactions(
  hash PK,
  block_number,
  tx_index,
  from,
  to,
  nonce,
  value,
  gas
)

receipts(
  tx_hash PK/FK,
  status,
  gas_used,
  contract_address
)

logs(
  block_number,
  tx_hash,
  log_index,
  address,
  topic0, topic1, topic2, topic3,
  data
)
```

推奨 INDEX:
* `(address, topic0, block_number desc)`

型:
* hash / topic は **bytea**
* `block_number` は **bigint**
* hex 文字列は **容量と index 効率が悪い**

---

## 4.5 DDL（Postgres / partition 前提）

```
-- blocks
create table if not exists blocks (
  number        bigint primary key,
  hash          bytea not null,
  parent_hash   bytea not null,
  ts            bigint not null,
  tx_count      int not null
);

-- transactions (partitioned by block_number)
create table if not exists transactions (
  hash          bytea primary key,
  block_number  bigint not null,
  tx_index      int not null,
  "from"        bytea not null,
  "to"          bytea,
  nonce         bigint not null,
  value         numeric(78,0) not null,
  gas           bigint not null
) partition by range (block_number);

-- receipts (partitioned by block_number)
create table if not exists receipts (
  tx_hash       bytea primary key,
  block_number  bigint not null,
  status        smallint not null,
  gas_used      bigint not null,
  contract_address bytea
) partition by range (block_number);

-- logs (partitioned by block_number)
create table if not exists logs (
  block_number  bigint not null,
  tx_hash       bytea not null,
  log_index     int not null,
  address       bytea not null,
  topic0        bytea,
  topic1        bytea,
  topic2        bytea,
  topic3        bytea,
  data          bytea not null,
  primary key (block_number, tx_hash, log_index)
) partition by range (block_number);

-- example partitions (100k blocks per partition)
create table if not exists logs_0_100k
  partition of logs for values from (0) to (100000);
create table if not exists txs_0_100k
  partition of transactions for values from (0) to (100000);
create table if not exists receipts_0_100k
  partition of receipts for values from (0) to (100000);

-- indexes
create index if not exists logs_addr_topic_block
  on logs (address, topic0, block_number desc);
create index if not exists txs_block
  on transactions (block_number, tx_index);
create index if not exists receipts_block
  on receipts (block_number);
```

補足:
* `value` は 256bit を想定して `numeric(78,0)`  
* `hash/topic/address` は `bytea` で固定（hex文字列は避ける）

---

## 5) 取り込み実装のコツ（詰まり回避）

* **PostgREST 経由より DB 直結が速い**  
* `INSERT ... ON CONFLICT DO NOTHING/UPDATE` で **冪等**にする  
* 大量 insert は **バッチ**（数百〜数千行）  
  * 1行ずつは遅すぎる

---

## 6) ワーカー設計（poll / 並列 / リトライ / バッチ）

### 6.1 poll 間隔（推奨）

* 基本: **1〜3秒**  
* head が動かないときは **指数バックオフ**（最大 30s）
* head が進んだら即時で追従

### 6.2 並列化の方針

* **1ワーカー = 1 cursor**が最も安全  
* スケールが必要なら **レンジ分割**（例: 1000 blocks 単位）  
  * ただし順序依存のある集計は **単一カーソル**で維持

### 6.3 リトライ

* 失敗は **同一 cursor で再試行**  
* 失敗回数が閾値を超えたら **一時停止**してアラート  
* 例外: `export_blocks` が `Pruned` を返した場合  
  * `cursor < pruned_before_block` なら **手動介入**が必要

### 6.4 バッチ戦略

* export は **max_bytes** で刻む  
* DB 書き込みは **数百〜数千行単位**  
* `INSERT ... ON CONFLICT DO NOTHING/UPDATE` で冪等化

---

## 7) エクスポート API 仕様（BlockBundle / cursor）

### 7.1 API 形（pull）

* `get_head() -> u64`
* `export_blocks(cursor, max_bytes) -> { chunks, next_cursor }`

### 7.2 cursor の意味（固定）

* `cursor: opt Cursor`
* `cursor = null` は **最初から**  
  * `oldest_exportable_block` から開始  
  * 例: `oldest_exportable_block = pruned_before_block + 1`

### 7.3 BlockBundle（最小）

```
BlockBundle {
  block_number: u64,
  block: bytes,         // block header + tx_ids などの本体
  receipts: bytes,      // receipts一式（同一ブロック分）
  tx_index: bytes       // tx_id -> (block, idx)
}
```

* **同一ブロック単位で完結**すること  
* `block` と `receipts` は **必ず一致する範囲**で返す

### 7.4 分割ルール

* `max_bytes` を超える場合は **cursor で分割**  
* 分割は **同一ブロック内のみ**で行う  
* 返却の `approx_bytes` は **実測値に近い概算**でよい

### 7.5 BlockBundle と Chunk の関係（v1）

* BlockBundle は **論理的に3 payload** を持つ  
  * `block`
  * `receipts`
  * `tx_index`
* export API は **Chunk 単位**で返す  
  * Chunk は **payload のスライス**であり、prefix は含めない

length-prefix 形式は **export API では使わない**。  
（将来のファイル保存形式などでのみ利用）

### 7.5.1 payload エンコード仕様（v1 固定）

**共通:**
* `tx_id` は **32 bytes 固定**（ハッシュの生bytes）  
* hex 文字列ではない  
* `len` は **u32be**（item の bytes 長）  
* `len = 0` は許容  
* `payload_len` も **u32 の範囲**

**block payload:**
* `block_bytes` **単体**  
* 形式: `bytes`（len prefix なし）

**receipts payload:**
* 同一ブロック内の receipts を列挙  
* 形式: `repeat { tx_id(32) + u32be(len) + bytes }`

**tx_index payload:**
* 同一ブロック内の tx_index を列挙  
* v1/v2 では **エントリ本体は 12 bytes 固定**（block_number: u64 + tx_index: u32）
* 取り込み側は len != 12 を検出したら **Decode 扱いで停止**（ブロック単位のスキップは禁止）
* 形式: `repeat { tx_id(32) + u32be(len) + bytes }`

**バージョニング:**
* export API は **v1 固定**（フィールド追加なし）  
* 破壊的変更は **新 API 名で切る**

### 7.6 cursor の詳細（Candid固定）

**wire互換を優先**し、enum ではなく **数値タグ**を固定する。

```
type Cursor = record {
  block_number: nat64;
  segment: nat8;      // 0=block, 1=receipts, 2=tx_index
  byte_offset: nat32; // payload 内 offset（prefix は含めない）
};

cursor: opt Cursor
```

固定値:
* `segment: nat8`
  * `0 = block`
  * `1 = receipts`
  * `2 = tx_index`
* `byte_offset: nat32`（payload 内 offset。prefix は含めない）

前提:
* 各セグメントの payload 長は `u32` で表現可能  
* v1 の DoS 対策として **max_segment_len（例: 8MiB）** を超える場合は reject

### 7.7 分割ルール（固定）

* `byte_offset` は **payload 先頭からの offset**
* 次セグメントへ進む場合:
  * `segment += 1`
  * `byte_offset = 0`
* `segment == 2` を完走したら:
  * `block_number += 1`
  * `segment = 0`
  * `byte_offset = 0`

分割返却の単位:
* `Chunk { segment: nat8, start: nat32, bytes: blob, payload_len: nat32 }`
  * `start` は payload 内 offset
  * `bytes` は payload の範囲（prefix は含めない）
  * `payload_len` は当該セグメントの全長（毎回返す）

### 7.8 cursor と Chunk の整合（固定）

* 返却される `chunks[0]` は **要求 cursor と一致**しなければならない  
  * `chunks[0].segment == cursor.segment`
  * `chunks[0].start == cursor.byte_offset`
* `chunks` は **同一 block_number** で **単調増加**（segment → start）
* `next_cursor` は **返却した最後の直後**（exclusive）を指す

連結可能性（必須）:
* 同一 segment 内では `next.start == prev.start + prev.bytes.len`
* segment が変わる時は `prev.start + prev.bytes.len == prev.payload_len`

### 7.9 返却範囲の固定（v1）

* **1レスポンスで block_number は最大1つ**
* segment は **block → receipts → tx_index の順で跨いでよい**
* もし将来「複数 block を同時返却」するなら  
  * Chunk に `block_number: nat64` を追加すること

### 7.10 バリデーション（必須）

* `segment > 2` → `InvalidCursor`
* `byte_offset > payload_len` → `InvalidCursor`  
  * `byte_offset == payload_len` は **有効**（完走状態）
* `start + bytes.len <= payload_len` を必ず満たす
* `payload_len <= max_segment_len`（例: 8MiB）
* `sum(chunks.bytes.len) <= max_bytes`  
  * `max_bytes` の上限は **1〜1.5MiB** を推奨

### 7.11 追いついた時の返し方（固定）

* **追いついている場合**: `chunks = []` かつ `next_cursor = cursor`  
  * indexer は同じ cursor で再試行できる

### 7.12 export API と BlockBundle の分離（明確化）

* export API は **Chunk のみ**を返す  
* BlockBundle（length-prefix 形式）は **保存/ファイル形式**に限定する

### 7.13 pruning 連携のエラー

* `cursor.block_number <= pruned_before_block` の場合は `Pruned { pruned_before_block }`

---

## 8) 運用/監視（メトリクス・アラート・バックプレッシャ）

### 8.1 推奨メトリクス

* `export_lag_blocks = head - cursor`
* `export_lag_seconds`（head の timestamp 差）
* `last_export_at`
* `export_rate_blocks_per_min`
* `db_write_latency_ms`
* `db_batch_size`
* `errors_per_min`

### 8.2 アラート例

* `export_lag_blocks > N` が **一定時間継続**
* `errors_per_min` が **連続で上昇**
* `db_write_latency_ms` が **閾値超え**

### 8.3 バックプレッシャ

* lag が増えたら **poll 間隔を短縮**  
* DB が詰まるなら **バッチサイズを縮小**  
* 取り込みが追いつかない場合は **ワーカー数を増やす**

---

## 6) まとめ

* Supabase Postgres は妥当  
* logs の partition は **最初からやる**  
* キューが必要なら **pgmq**  
* 定期実行は **pg_cron**  
* **timescaledb 前提は避ける**  

次に決めるべき設計軸は「logs のクエリ形」。  
`address + topic` の検索パターンが固まると、index 設計まで一気に決まる。

---

## 9) おすすめの実装順（現実に事故らない）

**Step 0: 保存基盤の固定（prune なし）**  
* BlobStore + alloc_table + free list を **通常保存の基盤**として先に導入  
* prune はまだしない（free() をほぼ使わなくても OK）  
* 目的は **再利用可能な器**を先に作ること

**Step 1: export API（cursor/chunk）実装**  
* 固定した Cursor / Chunk 仕様どおりに pull API を提供  
* **外部へ逃がす道**を先に用意する

**Step 2: indexer ワーカー + Supabase 格納**  
* export() を回して `next_cursor` で追従  
* DB には **冪等（UPSERT）**で書き込む  
* ここが安定しない限り prune は禁止

**Step 3: prune ガードを入れる（超重要）**  
* **手動フラグ方式（簡易）**: `pruning_enabled=false` を運用で明示的に ON

**Step 4: prune 実装（手動/テスト）**  
* `prune_blocks()` を完成させる  
  * Quarantine / 2-phase を含める  
* まだデーモンは有効化しない（`pruning_enabled=false`）

**Step 5: prune デーモン有効化**  
* `estimated_kept_bytes / stable_pages / last_prune_at` を監視  
* `max_ops_per_tick` を小さく始めて徐々に上げる


チェーンの正しさは canister 側、外部はキャッシュ（再構築可能）を徹底します。

全体方針

canister：canonical（ブロック生成・実行・最小履歴・export API・pruning）

indexer：pull型で export を吸い上げるだけ（落ちてもチェーンは進む）

保存：

検索用インデックス → SQLite（薄く）

生データ（payload） → Contabo Object Storage（zstd圧縮）

成長したら：インデックスを PostgreSQL に移す（rawはObject Storageのまま）

Phase 0: インフラ土台（1日）
0.1 Contabo VM

目安：4 vCPU / 8–16GB RAM / 1TB NVMe（まずは1台）

セキュリティ：SSH鍵、UFW、fail2ban、DBは外部公開しない

0.2 Object Storage（EU 1TBとか）

使い方：日次アーカイブ or ブロック範囲アーカイブを置く（DB用途ではない）

“ファイル名規則”を決める（例：chain=<id>/day=YYYY-MM-DD/part=0001.zst）

Phase 1: Indexer v1（SQLite + Archive）（2〜4日）
1.1 プロセス構成

indexer（単一プロセスでOK）

ループ：export_blocks(cursor, max_bytes) を呼ぶ

受け取った chunks を連結して segment payload を復元

payload をあなたの indexer-v1.md の仕様で decode

SQLite に “薄いインデックス” を upsert

raw payload は zstd 圧縮して一旦ローカルに書いて、まとめて Object Storage に upload

1.2 SQLite “最小スキーマ”（容量を抑える）

入れるものだけ決める（最初はこれで十分）：

meta

key TEXT PRIMARY KEY, value BLOB/TEXT

cursor（最重要）、schema_version、last_head、last_ingest_at

blocks

number INTEGER PRIMARY KEY

hash BLOB(32), ts INTEGER, tx_count INTEGER

txs

tx_hash BLOB(32) PRIMARY KEY

block_number INTEGER, tx_index INTEGER

from BLOB(20), to BLOB(20) NULL

status INTEGER, gas_used INTEGER（最小）

archive_parts

block_from INTEGER, block_to INTEGER

object_key TEXT, codec TEXT, size_bytes INTEGER, sha256 BLOB(32)

どのrawがどこにあるかの索引

logsは最初入れない（必要になったら “特定address/topic0だけ” のテーブルを追加）。

1.3 SQLite運用設定（最低限）

WALモード、定期checkpoint

取り込みはトランザクションでまとめる（例：Nブロック単位）

インデックスは最小限（txs(block_number)、txs(from)、必要ならtxs(to)くらい）

1.4 再開・冪等

cursor はコミット後に更新（SQLiteトランザクションと同じタイミング）

同じブロックを再取り込みしても UPSERT で壊れない

Phase 2: 可観測性（半日〜1日）

最低限これだけは出す（stdout→ログでもよい）：

ingest_blocks_per_min

cursor_lag = head - cursor.block_number

raw_bytes/day, zstd_bytes/day, sqlite_growth/day

エラー分類：Pruned, InvalidCursor, Network, Decode

これで「容量見積もり」と「追いついてるか」が数値で確定する。

Phase 3: Pruning の有効化準備（1〜2日）

あなたはすでに canister 側の BlobStore / Quarantine/Free を作ってるので、外部DBに依存しない形で進める：

3.1 Prune方針（外部に左右されない）

通常：retain_days と target_bytes で prune する

ただし運用安心のために emergency を分ける

high_water 超えたら prune tick

hard_emergency 超えたら aggressive prune（止まるよりマシ）

3.2 “外部ACKを必須にしない” 代わりにやること

indexerが死んでてもチェーンは進む

ただし「履歴が外に無い期間」が出る可能性は受け入れる

その代わり、Object Storageへのアーカイブが回ってるかだけ監視（これが現実的）

Phase 4: Explorer/API（必要になったタイミングで）

最初は indexer VM 上で軽いHTTP（読み取り専用）を出す

UIで必要なクエリは SQLite でも余裕で回ることが多い

Phase 5: Postgresへ昇格（必要条件が揃ったら）

「いつ Postgres にする？」のトリガは性能じゃなく運用要件：

Explorerを公開して同時アクセスが増えた

分析クエリが増え、JOIN/集計が重くなった

indexer/workerを複数にしたい

バックアップや運用をちゃんとやる覚悟が固まった

昇格手順（破壊しない）

rawはObject Storage継続（DBに入れない）

SQLite → Postgres に “インデックスだけ” 移行

indexerの書き込み先を Postgres に切り替え（cursorは同じ）

期限感の目安（雑に）

Phase 0〜2：1週間以内に「動く・追いつく・容量が見える」

Phase 3：数日で prune を安全にON（ただし最初は弱く）

Phase 5：必要になったら（最初からやらない）

最初に決め打ちしておくべき“固定値”

max_bytes（export取得上限）：1〜1.5MiB

圧縮：zstd（レベルは低めでOK、まず速度優先）

アーカイブ粒度：まずは 日次（後でブロック範囲にしてもよい）

SQLite保持期間：インデックスは30〜90日、rawはObject Storageで長期
---

# Appendix: Indexer Worker v2 (SQLite-first) 運用仕様（実装ブレ防止）

この章は **取得側（外部ワーカー）**の最小仕様を固定する。canister 側の export API 仕様（Cursor/Chunk/validation）は既存章に従う。

## v2.1 目的（固定）

* export API を pull して **外部インデックス（SQLite）**を構築する
* 外部DBは キャッシュ（失っても canister から再構築可能）
* pruning は外部ACKに依存しない（canister は単独で生存できる）

## v2.2 永続 cursor 形式（固定）

cursor は JSON で保存し、互換性と可読性を固定する。

* DB meta テーブルの cursor キーに JSON bytes を保存する
* JSON 内の構造は Cursor { block_number, segment, byte_offset } とし、CandidのCursor recordと同じ意味を持つ

```
Cursor {
  v: 1,
  block_number: "u64(文字列)", // JSの安全範囲超えを避けるため文字列固定
  segment: u8,                // 0=block, 1=receipts, 2=tx_index
  byte_offset: u32            // payload offset (prefixは含めない)
}
```

補足:

* cursor が存在しない場合は None とみなす（初回同期）
* block_number は **10進ASCII、先頭0なし**（"0" は許可）
* segment は **0/1/2 のみ**
* byte_offset は **0..=u32**

理由: JSの安全整数範囲（2^53-1）を越えたときのサイレント破壊を避ける。

## v2.3 Pull ループのコミット境界（固定）

取得側は以下を 不変条件として守る。

* cursor 更新は SQLiteのトランザクション COMMIT と同じ境界でのみ行う
* 取り込み（UPSERT）に失敗した場合は cursor を進めない
* next_cursor は export 返却のものをそのまま採用（exclusive）

## v2.4 追いつき時の扱い（スピン禁止・固定）

canister の仕様として、追いつき時は chunks=[] かつ next_cursor=cursor を返す。

取得側はこのケースで 必ず sleep/backoff し、スピン（busy loop）を禁止する。

* 初期 sleep: 200ms
* バックオフ: 指数（例: x2）
* 上限: 5秒固定
* chunks=[] が続く限り、sleep を挟んで再試行する

## v2.4.1 1レスポンス1ブロックの検知（固定）

export_blocks は **1レスポンス1ブロック**である。取得側はこれを前提とし、次を満たさない返却は InvalidCursor 相当として停止する。

* chunks は **単調増加**（segment → start）
* next_cursor.block_number は cursor.block_number または cursor.block_number + 1
  * +1 以外は “ブロック跨ぎ” の可能性が高いので停止

## v2.5 エラー分類と停止条件（固定）

取得側は以下のエラー分類を持つ。

* Pruned: 停止（fatal）
  * 理由: 要求範囲が canister 側で prune 済みであり、外部が欠けた状態からの回復手段が無い（cursor を進めても過去データは復元不能）
* InvalidCursor: 停止（fatal）
  * 理由: 取得側のバグまたは仕様不一致
* Decode: 停止（fatal）
  * 理由: 破損データまたは実装バグ。再試行で改善しない
* Net/Timeout: 再試行（retry）
  * backoff の対象（v2.4 と同じ上限 5秒）

## v2.6 最小スキーマ（v2）

v2 の最小スキーマは「追いつく」「復旧できる」「容量が測れる」に必要な最小に限定する。

* meta(key PRIMARY KEY, value)
  * cursor（JSON）
  * schema_version
  * last_head
  * last_ingest_at
  * last_error（任意）
* blocks(number PRIMARY KEY, hash?, timestamp, tx_count)
* txs(tx_hash PRIMARY KEY, block_number, tx_index)
* metrics_daily(day PRIMARY KEY, raw_bytes, compressed_bytes, sqlite_growth_bytes, blocks_ingested, errors)

※ metrics_daily は v2.1 でも **最小更新を入れる**（raw_bytes / blocks_ingested / errors）。

※ from/to/status/gas_used 等は v2.1 では必須にしない（必要になった時点で拡張する）。

## v2.7 アーカイブ（任意だが推奨）

* v2.1 は SQLite のみで開始してよい
* ただし pruning 自動化をONにする前に、少なくともローカル zstd での raw アーカイブを導入することを推奨する
* 目的: prune 後の調査・再構築・障害対応のための保険

## v2.8 Pruning デーモンの安全弁（固定）

pruning を外部ACKに依存させない設計とする代わりに、canister 側で 2段階の水位を持つ。

* high_water: 通常 prune 開始
* hard_emergency: 生存優先（retain を無視して古い方から削る）

推奨値:

* hard_emergency_ratio = 0.95（最初は 0.93 でも可）
