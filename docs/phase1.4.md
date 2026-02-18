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

**仕様固定（重要）**: `target_bytes` の判定に使うのは **estimated_kept_bytes**（保存時の積算）。  
`stable_pages` は **参考（アラート・検知用）**に留める。

**重要**: IC の stable memory は **一度 grow したら基本的に縮まらない**。  
そのため `target_bytes` は「今のサイズを減らす」指標ではなく、  
**これ以上 grow させないための圧力**として効く。  
古いキーを消す意味は、**次の書き込みで再利用できる領域を作る**ことにある。
prune は **キー削除**ではなく **Blob領域の回収（free list）**を伴うこと。  
これにより `target_bytes` が **grow抑止**として実際に効く。

### 2.1 目安の計算（概算でOK）

必要なのは「平均1ブロックあたりのバイト数」。  
（block本体 + receipts/logs + tx_index/loc 等）

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
  `auto-mine` 前後の stable ページ差分を観測して平均化

**注意（推定誤差）**  
ここで得られる値は **推定**であり誤差がある前提で運用する。  
過小評価を避けるため、**安全側（やや大きめ）**に見積もる。

補足: stable pages の差分観測は **補助メトリクス**に留める。  
推定の主体は **保存時のサイズ積算**とし、  
`estimated_kept_bytes` として運用に使う。

### 2.3 実務上の初期値（500GB前提）

* まずは **14日分**を狙う（運用が一気に楽）
* 余裕があれば **30日**
* きついなら **7日**

最終的には **容量ベースで自動調整**に移行する（最強）。

---

## 3) pruning 実装: 自動 prune デーモン + 手動は緊急用

Phase1.4では **自動化を薄く入れる**のが正解。理由：

* 「消し忘れ＝チェーン死亡」になる運用はプロダクト側で吸収すべき
* 自動化は **ポリシー設定 + 呼び出し**に限定し、暴走を防ぐ

### 3.1 追加する状態（最小）

* `pruning_enabled: bool`
* `policy: PrunePolicy { target_bytes, retain_days, retain_blocks, headroom_ratio, max_ops_per_tick }`
* `high_water_bytes / low_water_bytes`（ヒステリシス用・例: 0.90 / 0.80）
* `pruned_before_block: Option<u64>`（既存）
* `prune_cursor: u64`（次に消すブロック）
* `prune_running: bool`（再入防止）
* `oldest_kept_block: u64`（= `pruned_before_block + 1`）
* `oldest_kept_timestamp: u64`（`oldest_kept_block` の timestamp をキャッシュ）

### 3.2 自動実行の仕組み（IC的に安全）

* block event（84 blockごと）で `prune_tick()` を呼ぶ  
* `auto-mine` の中では行わない（重くて詰む）
* `prune_tick()` は **1回で少しだけ**進める  
  → `prune_blocks(retain, max_ops)` を1回呼んで終了
* `max_ops_per_tick` で 1tick の仕事量を固定（暴走防止）

### 3.3 should_prune() の判定（単純化）

トリガは2系統のみ：

* **保持期間**: `oldest_kept_timestamp < now - retain_days`
* **容量**: `estimated_kept_bytes > high_water_bytes`
  * prune 後は `<= low_water_bytes` まで戻す（ヒステリシス）

`estimated_kept_bytes` は 2.2 の積算値を使う。  
stable pages 観測は **参考**に留める。

retain の優先順位（コード化）:

* `retain_min_block = max(head - retain_blocks, block_at(now - retain_days))`
* `retain_days` が無い/0 の場合は `retain_blocks` のみ
* `retain_blocks` が無い/0 の場合は `retain_days` のみ
* **容量トリガが発動したら retain を無視**して古い方から削る

### 3.4 prune_blocks(retain, max_ops) の仕様（必須）

* **冪等**（何回呼んでも壊れない）
* **max_ops** は「削除エントリ数の合計」  
  （blocks + receipts + tx_index + tx_locs を全て数える）
* **部分実行**できること（時間超過対策）
* **1ブロック原子**で削除する  
  * 次のブロックを **消し切れないなら着手しない**
* 安全な削除順序（固定・2フェーズ）  
  1) 対象ブロックのメタから `tx_ids / receipt_ptrs / tx_index_ptrs` を読む  
  2) それらの `BlobPtr` を **Quarantine** に遷移（再利用禁止化）  
  3) map の参照エントリ（block/receipt/index）を削除  
  4) ここまで成功したら **Quarantine → Free** に遷移（free_list に戻す）  
  5) 成功時のみ `pruned_before_block` / `prune_cursor` を進める  
  6) `estimated_kept_bytes` を **class 単位で減算**（Used→Free のときのみ）

Quarantine を挟む理由:
「参照が残った状態で領域が再利用される」事故を防ぐため。  
Quarantine 中は allocate 対象に入らない。

PruneJournal（復旧）:
* prune の途中状態は **PruneJournal** に記録し、再実行で Free まで到達できること  
* `pruned_before_block` は **完了済みブロックまで**しか進めない

### 3.5 失敗時のルール（明文化）

* `pruned_before_block` は **完了したブロックまで**更新  
  （途中失敗時は “最後に完了した番号” を保持）

### 3.6 API（運用に必要な最小）

* `set_prune_policy(policy)`（target/retain/headroom/interval）
* `set_pruning_enabled(bool)`（キルスイッチ）
* `prune_now(max_ops)`（緊急・テスト用）
* `get_prune_status()`  
  * `pruned_before_block / estimated_kept_bytes / stable_pages / need_prune / last_prune_at`
* `dry_run` は任意（入れるなら `prune_plan()` で概算のみ返す）

need_prune の意味（監視用途）:
* `pruning_enabled` を **無視**して判定する  
* **時間トリガ（retain_days）**または **容量トリガ（target_bytes / high_water 超え）**で true  
* 実際に prune を実行するのは `pruning_enabled == true` のときのみ  
  * `need_prune == true` でも enabled=false なら prune は走らない（アラート用途）

---

## 4) prune 後に守るべき不変条件（データ整合）

* `block -> tx_ids` は必ず残る  
* `tx_index / receipts / tx_locs` は **残存ブロック内の tx_id のみ**残す  
* どれか1つだけ残る状態を作らない

これを **レビュー基準**として固定する。

---

## 4.5 外部 indexer への export（pull 前提）

**最も現実的な構成:** indexer が **pull** で取りに来る。  
canister は **エクスポートAPIを提供**するだけ。

利点:
* canister からの HTTP outcall が不要（安い・壊れにくい）
* 再試行が簡単（同じ cursor で取り直せる）
* 取り込み速度を indexer 側で制御できる（バックプレッシャ）

API 形（v1）:

* `get_head() -> u64`
* `export_blocks(cursor: opt Cursor, max_bytes: nat32) -> { chunks, next_cursor }`

返却は **Chunk 単位**であり、**1レスポンスにつき最大1 block_number**。  
segment は数値タグ固定:
* `0 = block`
* `1 = receipts`
* `2 = tx_index`

`byte_offset` は **payload 内 offset**（prefix は含めない）。

サイズ上限（安全側）:
* Ingress payload 最大 **2MiB**
* 返信も **2MiB前提で分割**が無難  
* `max_bytes = 1_000_000〜1_500_000` を推奨

### 追いつき時（catch-up）

要求 `cursor.block_number > head` のとき:
* `chunks = []`
* `next_cursor = cursor`（そのまま返す）

### チャンク整合（固定）

* `chunks[0]` は要求 cursor と一致（cursor がある場合）  
* `chunks` は同一 block_number 内で **単調増加**（segment → start）  
* `next_cursor` は返却した最後の直後（exclusive）

詳細な wire 仕様（Cursor/Chunk の型、分割ルール、validation）は **indexer-v1.md を正**とする。

### pruning と export の関係（方針A: 外部DBはキャッシュ）

* **チェーンの正しさ/進行は外部DBに依存しない**
* pruning は **target_bytes / retain_days のみ**で決める
* indexer/外部DBは **再構築可能な派生データ**

この方針では **exportのACKに依存しない**。  
落とし穴は「indexerが落ちている間に prune が進むと、外部履歴が欠ける」点。  
これは **観測性の損失**として許容する。

注意:
* Candid のオーバーヘッドがあるため、上限ギリギリは避ける  
* push（outcall）する場合は `max_response_bytes` を小さく固定（例: 4KB〜16KB）

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
* `Unavailable` は将来の拡張余地（Phase1.4 では不要）

pruning は **データ可用性の仕様**であり、削除された範囲は API で明確に区別される（`Pruned` を返す）。
