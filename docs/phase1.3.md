A) 最低ガス代要求（inclusion policy）

## 1) 単位・型・算術（最初に固定）

単位は **wei 固定**（gwei は UI/運用表示だけ）。

型は **u128**。ただし **すべて checked/saturating を必須**（panic禁止）。

特に乗算は事故るので、以下を **仕様で強制**する：

* `gas_limit * effective_gas_price` は `checked_mul`
* 合算は `checked_add`
* `base_fee_per_gas + max_priority_fee_per_gas` は `checked/saturating`（panic禁止）

「仕様で強制」を明記して、`as u64` などの雑な縮小を防ぐ。

ChainConfig を stable に持つ：

block_gas_limit: u64

min_priority_fee_per_gas: u128（EIP-1559用、tip下限）

min_gas_price_legacy: u128（legacy用下限。min_priority_feeとは分離）

base_fee_per_gas: u128（**固定ではなく可変**。0は禁止）
initial_base_fee_per_gas: u128 = 1_000_000_000（**非0の初期値**。1 gwei）
elasticity_multiplier: u64 = 2（EIP-1559の目標ガス算出に使用）
base_fee_max_change_denominator: u64 = 8（EIP-1559の変動上限）

base_fee は「メインネット準備」として **EIP-1559の更新式**を採用する。
目標ガス: `target_gas = block_gas_limit / elasticity_multiplier`
更新式（u128でchecked/saturating、除算は切り捨て）:
```
if gas_used > target_gas:
  delta = base_fee_per_gas * (gas_used - target_gas)
          / target_gas / base_fee_max_change_denominator
  next_base_fee_per_gas = base_fee_per_gas + delta
else if gas_used < target_gas:
  delta = base_fee_per_gas * (target_gas - gas_used)
          / target_gas / base_fee_max_change_denominator
  next_base_fee_per_gas = base_fee_per_gas.saturating_sub(delta)
else:
  next_base_fee_per_gas = base_fee_per_gas
```

Tx の “有効ガス単価” を定義：

Legacy: effective = gas_price

EIP-1559: effective = min(max_fee_per_gas, base_fee + max_priority_fee_per_gas)

## 2) legacy / 1559 の整合と receipt までの一致

**effective_gas_price の定義は receipt の effective_gas_price と一致**させる。

EIP-1559 の定義はこれで固定：

```
effective = min(max_fee_per_gas, base_fee_per_gas + max_priority_fee_per_gas)
```

※ `base_fee + max_priority` は checked/saturating（panic禁止）

さらに inclusion 条件（最低要求）も明記：

* 1559: `max_priority_fee_per_gas >= min_priority_fee_per_gas`
* 1559: `max_fee_per_gas >= base_fee_per_gas + min_priority_fee_per_gas`
* legacy: `gas_price >= min_gas_price_legacy`

さらに inclusion条件として
max_priority_fee_per_gas >= min_priority_fee_per_gas
max_fee_per_gas >= base_fee + min_priority_fee_per_gas を要求（重要）

## 3) submit / reject と produce / drop の境界（Phase1.2と揃える）

**reject = submit 時点で判定できるためキューに入れない**  
**drop = キュー投入後、produce_block 時点で判定される失敗**

EIP-1559 の min fee 未満は **reject** に寄せる。

静的に弾けるものは submit 時点で弾く（後で持ち続けるのが無駄）：

gas_limit <= block_gas_limit

gas_limit >= intrinsic_gas(tx)（最低ガス）

上の min fee 条件

動的にしか分からないものは produce_block 時点で弾く：

balance >= value + gas_limit * effective

nonce == state.nonce(sender)（次で説明する「nonce詰まり回避」を入れると無駄が減る）

B) 手数料並べ替え（ordering）

## 4) FIFO固定との矛盾を解消する（Phase1.3の位置付け）

Phase1.2 は FIFO 固定なので、Phase1.3 で **並べ替え導入**を明示する。

選択肢A（おすすめ）: `ordering_mode` を導入して段階的に切替

* Phase1.2: `ordering_mode = FIFO`（既定）
* Phase1.3: `ordering_mode = FEE_SORTED` を追加
  * Phase1.3で切替えるなら default を **FEE_SORTED**
  * 互換性優先なら default を **FIFO** のまま、運用フラグで切替

単純に「キュー全体をソート」は、長期運用で重くなるのでやめた方がいい。代わりに **優先度キュー（実体は StableBTreeMap）**を作る。

ただし Ethereum 的に重要なのがこれ：

同一 sender の tx は nonce 順にしか実行できない。
nonceが飛んでる高手数料Txを先に拾っても実行できない。

なので、正攻法は：

「senderごとの nonce 待ち」＋「全体は fee で競争」

senderごとに pending_by_sender: Map<sender, BTreeMap<nonce, tx_id>>

全体の優先キューは「各 sender の 次に実行可能な tx だけ」を入れる
→ これで “実行できないTxを延々拾う” を防げる

全体順序キー（決定性必須）

primary: effective_gas_price DESC

tie-break: seq ASC（受理順の単調増加ID。絶対必要）

（必要なら）さらに tx_hash を tie-break に追加してもいい

データ構造（stableでやる現実解）

StableBTreeMap は「キーの辞書順」で並ぶので、降順にしたい値は反転させる。

例：キーを fee_inv || seq || tx_hash にする（全部 big-endian）

fee_inv = u128::MAX - effective_fee（小さいほど高優先）

seq は単調増加 u64

tx_hash は衝突回避（同一 fee/seq を避けたいなら不要だが安全）

## 6) ready_queue のキーと整合性ルール（実装事故を潰す）

キー仕様（決定性のため固定）：

* 固定長バイト列
* big-endian
* 辞書順 = 優先順
* 例: `(fee_inv, seq, tx_hash)` の順

整合性（不変条件）：

* 同一 tx_id は ready_queue と pending_by_sender に **二重に存在しない**
* 状態遷移を固定：
  * submit → pending_by_sender（nonce待ち）
  * sender の next_nonce 到達 → ready_queue へ昇格
  * included/drop → 両方から確実に削除
* 削除は **片側だけ残る状態を作らない**

必要なMapは最小で2つ：

ready_queue: StableBTreeMap<KeyBytes, TxId>（pop最小＝最大fee）

txid_to_key: StableBTreeMap<TxId, KeyBytes>（削除や更新用）

sender別 nonce待ちは：

pending_by_sender_nonce: StableBTreeMap<(sender, nonce), TxId>

pending_min_nonce: StableBTreeMap<sender, nonce>（そのsenderの最小nonceをすぐ取る）

## 5) block_gas_limit と tx_count_limit の優先順位（決定性の補強）

停止条件の **順序**を仕様で固定する（実装差で結果が変わるのを防止）。

推奨ループ条件：

* `tx_count < tx_count_limit` かつ
* `gas_used + tx.gas_limit <= block_gas_limit`（パッキング可能）

どちらか満たせない時点で終了。

同一入力なら同じ tx が入るように、**取り出し順序とパッキング判定**を明文化する。

produce_block の流れ（推奨アルゴリズム）

ready_queue から最高優先を pop

tx を読み出して dynamic_check：

nonce == current_nonce(sender) でないなら
→ 本来は ready_queue に載せない設計なので、ここに来たらバグ or race。drop ではなく “再構築” 寄り。

balance 足りない / base_fee 条件満たさない → drop_code（INVALID_FEE）

実行して Included / Dropped を記録

同 sender の current_nonce が進んだら、

pending_by_sender から「次nonce」の tx を見つけて ready_queue に push

block_gas_limit / tx_count_limit に達したら終了

これで「fee並べ替え」しつつ「nonceで詰まらない」。

drop_code / reject の追加（今の Phase1.2 ドキュメントに足すなら）

少なくともこれを追加すると運用が楽：

LOW_FEE（min fee未満）※実装は INVALID_FEE に統合してもよい
INVALID_FEE（base_fee 変動で有効手数料を満たさなくなった場合を含む）

GAS_LIMIT_TOO_HIGH

GAS_LIMIT_TOO_LOW（intrinsic未満）

INSUFFICIENT_BALANCE_FOR_GAS

NONCE_MISMATCH（基本は起きない設計にする）

“decode失敗”はそのまま 1 でOK。

REVM への反映（ガス代を実際に課金・検証したいなら）

Env.tx.gas_price に effective_gas_price を入れる

BlockEnv.basefee を base_fee_per_gas にする（可変）

1559 を入れるなら、REVM が basefee を参照して intrinsic / fee周りを整合させるので、“effectiveだけで誤魔化す”より basefeeもセットが安全

最短で入れるなら（段階案）

実装コスト順に：

min fee 条件だけ入れる（submitで弾く）

並べ替え（fee desc, seq asc）だけ入れる（ただし nonce詰まりでdrop増える）

senderごとのnonce待ち＋ready_queueに進化（ここで完成）

最終的に 3) まで行かないなら、手数料並べ替えは “見た目だけ” になりがち。

テスト（最低限）

同一 tx 集合・同一 seq → 常に同じ順序で inclusion される（決定性）

sender A の nonce 0/2 があっても nonce 2 は ready に上がらない（nonceゲート）

base_fee/min_tip を満たさない 1559 が確実に drop（コード一致）

queue_snapshot の cursor は **seq ではなく offset**（ready_queue の先頭からの件数）で扱う。

legacy の gas_price < min は submit で reject

## 7) 追加テスト（運用に効く）

* legacy/1559 の `effective_gas_price` が receipt まで一致する
* 1559 の min fee 未満が submit で reject（drop ではない）
