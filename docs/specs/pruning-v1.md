# Pruning 実装Spec v1（BlobStore + free list + prune デーモン）

## 目的

* prune を「キー削除」ではなく **Blob領域の回収**まで含めて定義する  
* `target_bytes` を **grow 抑止の圧力**として機能させる  
* prune を **小分け・安全・再実行可能**にする

## 前提（固定）

* stable memory は **一度 grow したら基本的に縮まらない**  
* `target_bytes` 判定に使うのは **estimated_kept_bytes**  
* `stable_pages` は **参考（アラート/検知）**のみ

---

## 1) データモデル（最小）

### 1.1 BlobPtr

```
BlobPtr { offset: u64, len: u32, class: u32, gen: u32 }
```

* `offset`: stable 上の開始位置  
* `len`: 実データ長（class より小さい可能性あり）  
* `class`: サイズクラス（再利用単位）
* `gen`: 世代番号（再利用安全性のための必須）

### 1.2 サイズクラス（固定）

* 64KB / 128KB / 256KB / 512KB / 1MB / 2MB / 4MB

要件:
* **class >= len** となる最小クラスを使う  
* 1クラスは **固定サイズ**（内部のフラグメントは許容）

### 1.3 BlobStore

* `arena_end: u64`（次の追記位置）
* `free_list`（class 別の空き領域）
* `alloc_table`（領域の世代/状態）

実装の最小（stable で完結・pop 戦略固定）:
* `free_list_by_class: StableBTreeMap<(class, offset), ()>`  
  * class ごとの空き領域を **キー順**で保持  
  * pop は `range((class, 0)..(class, u64::MAX))` の **最初（最小 offset）**を取る
* `alloc_table: StableBTreeMap<(class, offset), (gen, state)>`  
  * `state = Used | Quarantine | Free`  
  * **世代 + 状態**で二重 free と古い参照を防ぐ  
  * **Quarantine** は再利用禁止（参照削除完了までの隔離）
* coalesce（連結/圧縮）は v1 では **非対応**

---

## 2) 書き込み（allocate）

1) `class = smallest_class(len)`  
2) `free_list[class]` が空でなければ pop して使う  
3) 空なら `arena_end` から `class` サイズで新規領域を確保  
4) `arena_end += class`（必要なら stable grow）

世代・状態の更新:

* 既存領域の再利用:
  * `alloc_table[(class, offset)].gen += 1`
  * `state = Used`
* 新規確保:
  * `gen = 1`, `state = Used`

戻り値は `BlobPtr { offset, len, class, gen }`。

---

## 3) 削除（free）

* `alloc_table[(class, offset)]` を参照して **世代と状態を確認**する
* 条件:
  * `state == Free` → **no-op**（idempotent）
  * `state == Quarantine` → **no-op**（既に隔離済み）
  * `state == Used` かつ `gen != BlobPtr.gen` → **no-op / Err**（古い参照）
  * `state == Used` かつ `gen == BlobPtr.gen` → `state = Quarantine`

**Free への遷移は prune のコミット後のみ。**

これにより **古い参照が新しい割当を free する**事故を防ぐ。

v1 では **領域クリア（zeroing）**は不要。

---

## 4) prune_blocks の原子性（1ブロック）

**「次のブロックを消し切れないなら着手しない」**を具体化する。

順序（固定）:

1) 対象ブロックのメタから `tx_ids / receipt_ptrs / index_ptrs` を読む  
2) それらの `BlobPtr` を **Used -> Quarantine** に遷移（free_listには入れない）  
3) map の参照エントリ（block/receipt/index）を削除  
4) 成功時のみ `pruned_before_block / prune_cursor` を進める  
5) **Quarantine -> Free** に遷移して free_list に insert  
6) `estimated_kept_bytes` を **class 単位で減算**

この順序なら:
* 途中で止まっても **再実行で復旧可能**
* 「参照だけ残る」「参照だけ消える」事故を避けやすい

---

## 5) prune デーモン（timer）

### 5.1 状態（Phase1.4 と一致）

* `pruning_enabled: bool`
* `policy: { target_bytes, retain_days, retain_blocks, headroom_ratio, timer_interval, max_ops_per_tick }`
* `high_water_bytes / low_water_bytes`
* `pruned_before_block / prune_cursor`
* `prune_running: bool`
* `oldest_kept_block / oldest_kept_timestamp`

### 5.2 tick の流れ

1) `prune_running` が true なら return  
2) `should_prune()` が false なら return  
3) `prune_running = true`  
4) `prune_blocks(retain, max_ops_per_tick)` を **1回だけ**呼ぶ  
5) `prune_running = false`  
6) 次の tick を schedule

**max_ops_per_tick** が暴走防止の最小ガード。

---

## 6) should_prune（優先順位の実装）

トリガは **2系統**のみ:

* **保持期間**: `oldest_kept_timestamp < now - retain_days`
* **容量**: `estimated_kept_bytes > high_water_bytes`

retain の決定:

* `retain_min_block = max(head - retain_blocks, block_at(now - retain_days))`
* `retain_days == 0` → `retain_blocks` のみ  
* `retain_blocks == 0` → `retain_days` のみ  
* **容量トリガ発動時は retain を無視**して古い方から削る

---

## 7) 失敗/再実行の前提

* `pruned_before_block` は **完了済みのブロックまで**更新  
* 途中失敗は **次回の prune で再実行**  
* 旧ポインタは **世代不一致**で free できない
* 参照が残っていても **Quarantine は再利用されない**

---

## 8) 非目標（v1）

* stable の compaction / shrink  
* free list の coalesce  
* ブロック並列 prune

---

## 8.5 export 連携（prune の上限ガード）

外部 indexer が **pull** で export する前提を置く場合:

* `exported_before_block: Option<u64>` を保持
* indexer が `ack_exported(block)` で進捗を返す
* prune は **min(prune_cursor, exported_before_block)** までしか進めない

これにより「未exportのブロックが消える」事故を防げる。

---

## 9) 最低限のテスト

* 同一 class の allocate/free を繰り返すと **ある時点から grow が発生しない**  
  * または **grow 回数が上限以下**  
* prune の途中失敗 → 再実行で整合  
* 1ブロック原子の順序が守られる（参照だけ残らない）

---

## 10) estimated_kept_bytes の加減算（固定）

* allocate 時: `estimated_kept_bytes += class`
* free 時（Quarantine -> Free のときのみ）: `estimated_kept_bytes -= class`
