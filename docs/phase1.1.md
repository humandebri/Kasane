---

## 0) 大前提：決定性ルール（ここがブレると全部壊れる）

**timestamp / block_time は外部時計に依存させない。**
ICPの `time()` をそのまま使うとレプリカ間決定性が揺れる可能性があるので、ブロックの timestamp は **チェーン内部状態だけ** で決める。
現行実装は **`head.timestamp + 1`** で単調増加にしている（`auto-mine` と `execute_*` 両方）。
---

1) 自動ブロック生成（タイマー：一定間隔 + キュー非空なら実行）
1.1 目的

アイドル時コストをほぼゼロにする（heartbeatの“空振り”固定費を避ける）

「キューが非空のときだけ」ブロック生成を試行する

1.2 方式（仕様化）

ポーリング禁止、1回の起動で最大1ブロック、ガード付き。

トリガー：一定間隔タイマー（5秒）

タイマーは「一定間隔で起動するが、キューが空なら何もしない」方式にする。
空ブロックは作らない。

補助トリガー（任意）：submit時（enqueue時）に schedule_mining() を呼んでも良いが、
最終的な生成はタイマーに委ねる（重複起動をガードで抑制）。

submit_* が tx をキューに入れた直後に schedule_mining() を呼ぶ

schedule_mining() の仕様：

if auto_production_enabled == false -> return

if is_producing == true -> return

if mining_scheduled == true -> return（重複スケジュール防止）

if queue_len == 0 -> return

mining_scheduled = true

一定間隔タイマーをセット（delay=5s、運用で変更できるようにconfig化）

タイマーハンドラ

mining_scheduled = false（最初に落とす）

if auto_production_enabled == false -> return

if is_producing == true -> return

if queue_len == 0 -> return

is_producing = true（先に立てて再入防止）

自動採掘（timer）を実行

is_producing = false

次回タイマーまで待つ（1回=最大1ブロック）

これで「キューがある限り連続で掘る」が、**ループではなく“イベント列”**として進む（1メッセージ=最大1ブロック）。

1.3 await と再入

auto-mine の内部で await を挟まないのが理想

もし跨ぐなら 状態機械化が必要（Phase1では避けるのが無難）

is_producing と mining_scheduled を stable に持つのは必須（upgrade/再入で壊れないようにする）

1.4 stableに持つ追加状態（修正版）

必須：

last_block_time: u64

is_producing: bool

auto_production_enabled: bool

追加（イベント駆動に必要）：

mining_scheduled: bool（ワンショット重複投入防止）

追加（運用/デバッグ）：

next_tx_seq: u64

last_produce_attempt: u64（任意）

1.5 “一定間隔 + 空キュー時スキップ”を採用（明記）

定期タイマーは採用するが、**キューが空なら必ずスキップ**する。
これにより「空ブロック連発」と「アイドル時の無駄」を避ける。

遅延は「最大5秒 + 実行時間」に抑える。低遅延が必要なら間隔を短くする。

---

## 2) pending可視化（mempool無し版）

ここは「**キュー**」を唯一のpendingとして扱うのがコア。

キュー順序は **FIFO（QueueMeta の head/tail）** を前提にする。
順序規則（価格順/優先度など）を変えるならここで仕様を更新すること。

### 2.1 tx_id の定義（ここが最重要）

`tx_id` を何にするかで観測APIの価値が決まる。

おすすめ：

* Route A（raw Ethereum tx）: `tx_id = keccak256(raw_tx)`（= Ethereum tx hash と一致）
* Route B（IC合成Tx）: `tx_id = keccak256( domain_sep || caller_principal || nonce || payload )`

  * `domain_sep` を固定文字列で入れる（衝突回避・将来拡張）

こうすると Phase2で `eth_getTransactionByHash` に直結する。

### 2.2 状態モデル（最小で十分）

* `queue: VecDeque<QueuedTx { tx_id, kind, seq, ... }>`
* `tx_index: HashMap<tx_id, TxLoc>`（軽い索引。mempoolじゃない、ただの位置情報）
* mining_scheduled の存在により「今掘る予定か」を観測したいなら、将来 get_status() 的に返してもいい

`TxLoc` はこれで足りる：

* `Queued { seq, pos_hint? }`
* `Included { block_number, tx_index }`
* `Dropped { reason_code }`（任意だが超おすすめ）
* `Unknown`

Droppedが無いと “いつまでもQueuedっぽいのに実は消えた” を説明できない。

### 2.3 API案（そのまま採用でOK、返しだけ固める）

* `get_pending(tx_id) -> Queued | Included(block_number, index) | Dropped(reason?) | Unknown`
* `get_queue_snapshot(limit, offset) -> items[]`

  * `items[]: { tx_id, kind, seq }`
  * `enqueued_at_block` は無理に要らない。`seq` が単調増加なら “順序” は説明できる。

`offset` は安易に `VecDeque` をスキャンするので O(n) になる。そこで**ページング用カーソル**に寄せると強い：

* `get_queue_snapshot(limit, cursor_seq?) -> { items, next_cursor_seq? }`

実装がシンプルなのにスケールする。

---

## 3) logs保存（索引なし）

これは “後で効く” じゃなくて **今から必須**。同意。

### 3.1 receipt最小仕様（EVM互換の要点だけ）

1 tx の実行結果として stable に残すべきは：

* `status`（success/fail）
* `gas_used`
* `cumulative_gas_used`（ブロック内の累積。互換を気にするなら）
* `logs: Vec<Log { address, topics[], data }>`
* `return_data`（あなたが言ってた ExecResult 拡張の中核）
* `contract_address`（create時）
* `effective_gas_price`（Phase1は0固定 or min_gas_price を使う。ChainStateに base_fee/min_gas_price を持たせて足場にする）

索引なしで良いので、保存構造は：

* `blocks[block_number].txs[i].receipt = Receipt{...logs...}`

で終わり。

### 3.2 REVMからの取り出し

REVMの実行結果（ExecutionResult / ResultAndState 等）から logs を収集して receipt に詰める。
ここは「ログを正しく保存する」以外に地雷は少ない。**ABIデコードやフィルタリングは一切しない**。

---

## 4) finalityモデル（reorg無し宣言）

これは仕様の一行じゃなく、**APIの意味**に影響するので、少しだけ丁寧に書くと後で揉めない。

### 4.1 specに入れる文言（そのまま使える）

* **Finality**: This chain is single-sequencer and does not support forks. A block produced by `auto-mine` is final and will never be reverted (no reorg).

### 4.2 影響を書く（短く）

* `block_number` は単調増加
* `Included(block_number, index)` は不変
* `Receipt` は不変
* “pending -> included” 以外の遷移は `Dropped` のみ（任意）

---

## 5) auto-mine の仕様をもう一段だけ固める（バグ予防）

### 5.1 失敗Txをどう扱うか

EVMは “Tx失敗してもブロックには入る（status=0）” が普通。
なので：

* **実行失敗 = ブロックから除外しない**
* receipt に status=0 と return_data/revert_reason（あれば）を保存
* state は “失敗ならrevert済み” なので commit しない（REVMがやる）

これを仕様に書くと、後で pending の整合が崩れない。

### 5.2 ガス上限・ブロックガス

Phase1なら簡略化して良い：

* `block_gas_limit` 固定
* `max_txs_per_block` と併用して “止まる” ようにする

---

## 5.3 ガスの実運用ルール（Phase1.1 固定）

* base_fee は **0（wei）固定**。運用で更新できるが、自動調整はしない
* legacy tx は **受け入れて変換**する
  * `max_fee = gas_price`
  * `max_priority = gas_price`
* EIP-1559 風の有効価格は次で決める
  * `effective = min(max_fee, base_fee + max_priority)`
  * `base_fee + max_priority` は **checked/飽和**で計算（panic禁止）
* 拒否条件
  * `max_fee < base_fee`
  * `max_priority > max_fee`
* receipt に `effective_gas_price` を保存
* **キュー順序はFIFO維持**（並べ替えは将来に回す）

---

## 6) 最小のインタフェース案（Canister APIとして）

Phase1で揃えるならこのセットが美しい：

* `submit_raw_tx(bytes) -> tx_id`
* `submit_ic_tx(args) -> tx_id`
* （旧案・廃止）`手動採掘API(max_txs) -> block_number`
* （旧案・廃止）`自動採掘のON/OFF API`
* `get_pending(tx_id) -> ...`
* `get_queue_snapshot(limit, cursor?) -> ...`
* `get_block_by_number(n) -> Block { header, tx_ids, ... }`
* `get_receipt(tx_id) -> Receipt?`

`scripts/playground_smoke.sh` runs the same scenarios (cycle tracking + block/tx probes + raw eth tx creation) for the playground canister.

Phase2のJSON-RPCは、結局これらの薄いラッパーになる。

---

## 7) 実装上の注意（これ踏むと死ぬ）

* **stable構造は “追記/バージョニング” 前提で設計**（upgradeで壊さない）
* `is_producing` は upgrade 中に true のまま死ぬ可能性があるので、`post_upgrade` で false に戻す（仕様にしてよい）
* `queue_len > 0` 判定と `pop` の間で状態が変わる可能性は、単一スレッドでも `await` があると起こる。なので `auto-mine` は基本 `await` なし（または状態機械）
