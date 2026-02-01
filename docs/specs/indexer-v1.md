# Indexer 実装Spec v1（pull + Supabase）

## 結論：おすすめ構成（現実に回るやつ）

1) 取り込みは **外部ワーカー** が pull  
2) canister の `export_blocks(cursor, max_bytes)` を **定期ポーリング**  
3) 取得分を Supabase Postgres に **直結で INSERT/UPSERT**  
4) 進捗 cursor は **DB に保存**（落ちても復帰可能）

Edge Functions だけでも可能だが、EVM logs が増えると **CPU/メモリが先に詰まる**。  
**常駐ワーカー（Rust/TS）**が最も安定。

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
