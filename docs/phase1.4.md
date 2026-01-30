<!-- pruningについて -->
1) 目標は「ブロック数」じゃなく「保持期間」

L2っぽい運用（sequencer + indexer）なら 7〜30日分が現実的な落とし所

監査/不具合調査をしっかりやるなら 30日あると楽

逆にインデクサが堅牢なら 7日でも回る

“challenge window がある本気ロールアップ”なら、少なくとも finalized（または異議期間経過）までは入力/証跡が追える必要がある。そこだけは削れない。

2) 500GB制約なら「容量で自動調整」が一番安全

retain_blocks = N固定 ではなく

target_bytes = 500GB を超えたら古いものから prune して戻す

これならブロックが急に太っても落ちない。

目安の計算（ざっくりで十分）

必要なのは「平均1ブロックあたり何バイト持ってるか」です（block本体 + receipts/logs + tx_index/loc 等の合計）。

ブロックあたりサイズ別：500GBで持てるブロック数

（ヘッドルーム20%取って 実効400GB で計算すると安全）

100KB / block → 約 4,000,000 blocks

200KB / block → 約 2,000,000 blocks

500KB / block → 約 800,000 blocks

1MB / block → 約 400,000 blocks

それを「何日分か」に変換（ブロックタイム別）

1秒ブロック：86,400 blocks/day

2秒ブロック：43,200 blocks/day

12秒ブロック：7,200 blocks/day

例：2秒ブロック & 500KB/block なら
800,000 blocks / 43,200 ≒ 18.5日分（十分現実的）

実装としての落とし所（あなたの設計に合うやつ）
A) “容量ベースprune” を仕様にする

metrics.total_history_bytes（推定でもOK）

metrics.pruned_before_block

prune_to_budget(target_bytes, max_ops) みたいにして、毎回少しずつ削る（一括は危険）

B) 1ブロックあたりサイズは「実測」で決める

やり方は2つ：

保存時にサイズを測る
block/receipt/tx_index をエンコードしたバイト長を足し上げて block_size_estimate[number]=... をリングに持つ

安定メモリの増分で測る（ページ差分）
「produce_block前後で stable メモリが何ページ増えたか」を見て平均化

これで「500GBに収まる保持期間」が自動的に見えてくる。

じゃあ「何ブロック持てばいい？」の即答（500GB前提の現実解）

まずは “14日分” を狙う（運用が一気に楽になる）

実測で余裕があるなら 30日分へ伸ばす

容量がきついなら 7日分へ落とす

そして最終的には **「容量ベースで自動調整」**に移行する（これが最強）

要するに：500GB縛りなら「ブロック数を固定しない」ほうが勝ちです。
「保持期間（7/14/30日）」を目標にして、実測の bytes/block から初期Nを置き、最終的に容量ベースpruneにすれば、運用でまず詰まりません。



3) pruning 実装：prune_blocks(retain, max_ops) 手動のみでOK？

OK。 Phase1.6で「自動prune無し」は妥当。理由：

自動pruneは運用ポリシーと密結合（いつ/どれだけ/どの条件で消すか）

まずは **安全な分割pruneの原語（primitive）**だけ入れるのが正しい

ただし、仕様・実装で最低限これだけは入れておくと運用が楽：

prune_blocks は 冪等（何回呼んでも壊れない）

max_ops は “削除したエントリ数” ベース（blocksだけじゃなく receipts/tx_index/tx_locs も数える）

任意だけど強い追加：

dry_run（どれだけ消えるか見積もり）

remaining（あと何ブロック分消し残ってるか）

---

## API仕様（prune / LookupError）

### metrics.pruned_before_block

* `pruned_before_block: Option<u64>`
  * `None`: prune未実施
  * `Some(x)`: **number <= x は取得不能**
* pruning を実行したら、この値を必ず更新する

### get_block / get_receipt の返り値

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

これにより **NotFound/Pending/Pruned** を確実に区別できる。
