---

# Phase1.2（運用・観測・検証の拡張）

## 1) 観測とアラート（最小セット）

### 1.1 収集する数値（query API）

* tip高さ / last_block_time
* queue長
* block生成レート（直近Nブロックから）
* 平均tx/ブロック（直近Nブロックから）
* reject率（drop_code別のカウント）
* cycles残量（get_cycle_balance）
* pruned_before_block（prune未実施なら None）

### 1.1.2 metricsの「返す値の定義」を固定する（必須）

* `ema_block_rate_per_sec_x1000`
  * 単位: **(blocks/sec) × 1000**（小数表現のため x1000 で固定）
  * `alpha`: **0.2**（指数移動平均の平滑化係数）
* `ema_txs_per_block_x1000`
  * 単位: **(txs/block) × 1000**
  * **生成したブロックのみ**を対象にする（空ブロックは作らない前提）
* `last_k_block_stats`
  * `tx_count`
  * `reject_count`
  * `timestamp`
* `reject_count_by_code`
  * **累積で単調増加**

同名メトリクスが別の意味を持たないように、ここを仕様として固定する。

### 1.1.1 queryで“直近Nブロック”を毎回走査しない

**禁止**: queryで直近Nブロックを都度走査して集計する方式。  
運用期間が伸びるとqueryコストが確実に上がるため、仕様で禁止する。

**推奨仕様**: `produce_block`（update）で集計を進め、queryは読むだけにする。

* `tip_height`
* `last_block_time`
* `ema_block_rate_per_sec_x1000`（指数移動平均）
* `ema_txs_per_block_x1000`
* `reject_count_by_code`（累積）
* `last_k_block_stats`（固定長リング: 例 K=64）

`metrics()` は **O(1) + O(K)** で返す。

### 1.2 ログ（update / timer で出す理由）

`produce_block` は **update 呼び出し**か **timerコールバック**でしか走らない。
query では状態更新ができないため、ログは update/timer に集約するのが自然。

* produce_block: start/end/tx_count/reject_count
* drop_code別のカウント
* upgrade/post_upgrade
* タイマー再スケジュール

### 1.3 アラート条件（運用スクリプトで十分）

* queue長が閾値超え
* tip停止（last_block_time が進まない）
* cycles低下（しきい値）
* reject急増（drop_codeの急変）

### 1.4 cycles残量の扱い

queryで毎回「正確な cycles」を取得できる前提にしない。  
**直近の update/timer で観測した値**を返す方式で十分。

---

## 2) フォールトモデルの明記（補足）

* finality: reorg無し
* queue順序: FIFO固定（QueueMeta head/tail）
  * **不変条件**: キュー順序は head/tail のみで決まり、手数料等の並べ替えは行わない
  * 将来、手数料並べ替えを導入する場合は **決定的キー**
    （`effective_gas_price DESC`, `seq ASC`）を仕様として固定する
* timestamp規則: **head.timestamp + 1**
  * `produce_block` を呼ばない間は進まない（空ブロックを作らない）
  * 空ブロックを生成してタイムスタンプだけ進める仕様ではない
* reject理由: drop_code を一覧化して仕様に固定
* upgrade中: is_producing/mining_scheduled は post_upgrade で false に戻す

drop_code（現行実装の一覧）:

* 1: decode失敗
* 2: 予約（実行失敗は **Included + status=0** として扱い、dropにはしない）
* 3: tx_store欠落（キューにあるが本体が無い）
* 4: caller不足（IcSyntheticでcaller_evm未保存）

拡張ルール:

* 既存コードの意味は変更しない
* 新コードは末尾追加のみ
* 未知コードは `Unknown(n)` として扱う

### 2.1 reject と drop の発生場所（用語固定）

* **reject**: submit 時点の拒否（キューに入らない）
* **drop**: キュー投入後の失敗（produce_block 時に判定）

reject/drop を混在させないことで、queue長・reject率の解釈を安定させる。

---

## 3) データ保持ポリシー（方針案）

Option 2（暫定案）: **最新 N ブロックのみ保持（pruning）**

* receipts/logs を無期限保持はしない
* pruning条件: `head_number - N` より古いものを削除
* tx_loc/tx_index の整合も同時に整理する
* export/snapshot の方式は別途決める（外部indexer前提なら prune 前に吸い出し）

※ この節は意思決定待ち。実装は保留。

### 3.1 具体案（設計だけ）

**目的**: 増え続ける receipts/logs/tx_index/tx_loc を一定サイズに保つ。

**保持範囲**:

* `blocks` は最新 N ブロックのみ残す
* `receipts` / `tx_index` / `tx_locs` は、残す `blocks` に含まれる tx_id のみ残す

**削除対象の決定**:

* `prune_before = head_number - N`
* `blocks` の `number <= prune_before` を削除対象
* そのブロック内 `tx_ids` を集め、対応する
  * `receipts.remove(tx_id)`
  * `tx_index.remove(tx_id)`
  * `tx_locs.remove(tx_id)`（Included/Droppedを含む）

**順序**:

1) 削除対象ブロックから tx_id を収集
2) receipts/tx_index/tx_locs を削除
3) blocks を削除
4) `head` は変更しない（最新は保持）

**注意点**:

* `get_block` / `get_receipt` は「古いものは `None`」が仕様になる
* `get_pending` の `Dropped` も prune で消える（旧txの追跡不能）
* `tx_store` / `seen_tx` の扱いは **方針固定が必要**（無期限は破綻）

**実装の最小形**:

* `prune_blocks(retain: u64, max_ops: u32)` を手動APIとして追加
  * 1回の削除上限を設ける（updateの時間超過対策）
  * `did_work: bool` / `remaining: u64` を返す
  * 何回呼んでも同じ結果（冪等）
* Phase1.2では **自動pruneはしない**（運用で明示呼び出し）

任意案:

* `maybe_prune()` を `produce_block` 後に**少しだけ**回す（分割GCとして）

**テスト観点**:

* prune後に古い `get_block` が None
* prune後に古い `get_receipt` が None
* 最新ブロック/receipt は残る

### 3.2 “None” の意味問題（重要）

`get_block` / `get_receipt` の結果が **None** だと以下を区別できない:

* NotFound（存在しない）
* Pending（未確定）
* Pruned（削除済み）

この曖昧さを避けるため、**Phase1.2では Result で返す**方針にする。

* `get_pruned_before() -> Option<u64>` を追加する
* もしくは `pruned_before_block` を `metrics` に含める
* あるいは内部APIだけでも `LookupError::Pruned` を返す

### 3.2.1 Phase1.2 で決める案（推奨）

* `metrics.pruned_before_block: Option<u64>` を追加
  * `None`: prune未実施
  * `Some(x)`: **number <= x は取得不能**（仕様として）
* **Candid API は Result にする**
  * `get_block(number) -> Result<BlockView, LookupError>`
  * `get_receipt(tx_id) -> Result<ReceiptView, LookupError>`
* `LookupError` の意味を固定
  * `Pruned { pruned_before_block }`: `requested_number <= pruned_before_block`
  * `Pending`: `tx_loc.kind == Queued`
  * `NotFound`: それ以外

### 3.3 tx_store / seen_tx の最小方針案

* `tx_store`: receipts/tx_index と同じ保持範囲で prune
* `seen_tx`: 直近の tx hash を固定長で保持（例: 10,000件）

---

## 4) テストの最低限（Phase1.2）

* 決定性: 同一tx列 → 同一state_root
* reject網羅: drop_code の各ケースが再現できる
* upgrade復帰: pre/post_upgrade で state が変わらない
* queue/tx_loc整合: Queued → Included/Dropped
* fork/reorgモデル: reorg無しを前提にした不変条件テスト

追加で欲しい運用テスト:

* metrics が全走査しない（レビュー基準でも可）
* reject_count_by_code が単調増加 + upgrade越しに保持
* timer再スケジュールの多重起動防止
* prune の冪等性

### 4.1 互換/差分テスト（次段階）

* 既存クライアント（ethers/viem/foundry）との互換差分
* 既存テストベクタとの差分

### 4.2 fuzz / differential（将来）

* 2実装で同一入力→同一出力の検証
* コストは高いので Phase2 以降に回す

---

## 5) RPC互換DTOのデコード規約

* `raw` と `hash` は常に返す
* `decoded` は **Option**（デコード成功時のみ `Some`）
* `decode_ok` は `decoded.is_some()` と一致させる
* `decode_ok == false` のとき **decoded以外の派生フィールドは出さない**

デコード失敗時に「部分的に壊れたTx」を返さず、**decoded が丸ごと欠落**する仕様に固定する。
