<!-- pruningについて -->

# Phase1.4（pruning計画の確定版）

## 0) 前提・用語（優先順位を固定）

* **target_bytes**: 最優先（容量上限）
* **retain_days**: 初期の目安（運用目標）
* **retain_blocks**: 暫定運用パラメータ

優先順位は **target_bytes > retain_days > retain_blocks**。  
最終的に容量ベースへ収束させる前提で設計する。

---

## 1) 目標は「ブロック数」ではなく「保持期間」

* L2運用（sequencer + indexer）なら **7〜30日** が現実的
* 監査/不具合調査を重視するなら **30日**
* indexer が堅牢なら **7日**でも回る
* “challenge window があるロールアップ”では **finalized/異議期間経過までは保持必須**

---

## 2) 500GB制約は「容量で自動調整」が最も安全

**固定 retain_blocks は危険**。  
`target_bytes = 500GB` を超えたら古いものから prune する方式が最も安定。

### 2.1 目安の計算（概算でOK）

必要なのは「平均1ブロックあたりのバイト数」。  
（block本体 + receipts/logs + tx_index/loc 等）

ブロックあたりサイズ別：500GBで持てるブロック数  
※ヘッドルーム20%確保 → **実効400GB**で計算

* 100KB / block → 約 4,000,000 blocks
* 200KB / block → 約 2,000,000 blocks
* 500KB / block → 約   800,000 blocks
* 1MB / block   → 約   400,000 blocks

ブロックタイム別の換算：

* 1秒ブロック → 86,400 blocks/day
* 2秒ブロック → 43,200 blocks/day
* 12秒ブロック → 7,200 blocks/day

例：2秒ブロック & 500KB/block  
800,000 / 43,200 ≒ **18.5日**

### 2.2 実測で「bytes/block」を推定する

**やり方は2つ：**

* **保存時にサイズを測る**  
  block/receipt/tx_index をエンコードして byte を積算  
  `block_size_estimate[number]` をリングで保持
* **安定メモリの増分で測る**  
  `produce_block` 前後の stable ページ差分を観測して平均化

これで「500GBに収まる保持期間」が自動で見えてくる。

**注意（推定誤差）**  
ここで得られる値は **推定**であり誤差がある前提で運用する。  
過小評価を避けるため、**安全側（やや大きめ）**に見積もる。

### 2.3 実務上の初期値（500GB前提）

* まずは **14日分**を狙う（運用が一気に楽）
* 余裕があれば **30日**
* きついなら **7日**

最終的には **容量ベースで自動調整**に移行する（最強）。

---

## 3) pruning 実装: prune_blocks(retain, max_ops) を手動で運用

Phase1.4では **自動prune無し**が妥当。理由：

* 自動pruneは運用ポリシーと密結合（いつ/どれだけ消すか）
* まずは **安全な分割pruneの原語**だけ入れるのが正しい

### 3.1 最低限の仕様（必須）

* **冪等**（何回呼んでも壊れない）
* **max_ops** は「削除エントリ数の合計」  
  （blocks + receipts + tx_index + tx_locs を全て数える）
* **部分実行**できること（時間超過対策）

### 3.2 失敗時のルール（明文化）

* `pruned_before_block` は **完了したブロックまで**更新  
  （途中失敗時は “最後に完了した番号” を保持）

### 3.3 任意だけど強い追加

* **dry_run**（どれだけ消えるか見積もり）
* **remaining**（残タスク量を返す）
* **max_blocks_per_call**（1回のブロック上限）

---

## 4) prune 後に守るべき不変条件（データ整合）

* `block -> tx_ids` は必ず残る  
* `tx_index / receipts / tx_locs` は **残存ブロック内の tx_id のみ**残す  
* どれか1つだけ残る状態を作らない

これを **レビュー基準**として固定する。

---

## 5) API仕様（prune / LookupError）

### 5.1 metrics.pruned_before_block

* `pruned_before_block: Option<u64>`
  * `None`: prune未実施
  * `Some(x)`: **number <= x は取得不能**
* pruning 実行時に必ず更新する

### 5.2 get_block / get_receipt の返り値

**Candid API は Result で返す:**

* `get_block(number) -> Result<BlockView, LookupError>`
* `get_receipt(tx_id) -> Result<ReceiptView, LookupError>`

**LookupError の意味を固定:**

* `Pruned { pruned_before_block }`
  * `requested_number <= pruned_before_block` の場合
* `Pending`
  * `tx_loc.kind == Queued` の場合
* `NotFound`
  * それ以外

これにより **NotFound / Pending / Pruned** を確実に区別できる。

### 5.3 JSON-RPC層へのマップ（将来）

**方針（固定）**

* `Pruned` → **error** を返す
  * 理由: `null` だと **NotFound/Pending/Pruned** が区別できず運用が詰む
  * 互換性より **原因の明確さ** を優先する
* `Pending` → `null` を返す（Ethereum互換に寄せる）

補足:
* 将来もし `null` を採用する場合は、**別APIで pruned_before_block を必ず返す**こと
