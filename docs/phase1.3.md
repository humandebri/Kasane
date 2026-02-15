A) 最低ガス代要求（inclusion policy）

## 追補（任意最適化の実装メモ）

- `tools/indexer/src/archiver.ts` の原子的書き込みは `write-file-atomic` に統一。
  既存の「既存ファイル再利用」分岐は維持し、重複投入時の挙動互換を保つ。
- `crates/evm-core/src/chain.rs` の `select_ready_candidates` は
  全件 `sort` から `BinaryHeap` による上位K抽出へ変更。
  返却前に従来と同一ルール（fee desc / seq asc / tx_id asc）で確定整列する。

## 1) 単位・型・算術（最初に固定）

単位は **wei 固定**（gwei は UI/運用表示だけ）。

型は **u128**。ただし **すべて checked/saturating を必須**（panic禁止）。

特に乗算は事故るので、以下を **仕様で強制**する：

* `gas_limit * effective_gas_price` は `checked_mul`
* 合算は `checked_add`
* `base_fee_per_gas + max_priority_fee_per_gas` は `checked/saturating`（panic禁止）

「仕様で強制」を明記して、`as u64` などの雑な縮小を防ぐ。

ChainConfig を stable に持つ：

実装上の既定値（`initial_base_fee_per_gas` や `min_gas_price_legacy` など）は
`crates/evm-db/src/chain_data/runtime_defaults.rs` に集約する。
現行の `min_gas_price_legacy` 既定値は `1_000_000_000` wei（1 gwei）。

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

PR8の署名検証責務境界は `docs/specs/pr8-signature-boundary.md` を正本とする。

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

## 4) 並べ替えは FEE_SORTED 固定（FIFOに戻さない）

Queue ordering は **FEE_SORTED 固定**。

tie-break（seq）と sender の nonce gate を **仕様として固定**する。

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

---

## IC Synthetic 仕様まとめ（Phase1.3）

### 1) IcSynthetic payload 形式（v2固定）

```
[version:1=0x02]
[to:20]
[value:32]
[gas_limit:8]
[nonce:8]
[max_fee_per_gas:16]
[max_priority_fee_per_gas:16]
[data_len:4]
[data:data_len]
```

* 数値は **big-endian**。
* `data_len` は `u32(be)`、`data_len <= MAX_TX_SIZE`。

### 2) 保存構造（stable）

stable には decode済みTxではなく **StoredTxBytes** を保存する。

```
version: u8 (=2)
kind: TxKind (0x01=EthSigned, 0x02=IcSynthetic)
raw: Vec<u8> (kind依存で解釈)
fee: FeeFields (max_fee/max_priority/is_dynamic)
caller_evm: Option<[u8;20]> (IcSynthetic必須)
canister_id: Vec<u8> (IcSynthetic必須, principal bytes)
caller_principal: Vec<u8> (IcSynthetic必須, principal bytes)
tx_id: TxId (下記)
```

**重要**: stableの `raw` は `kind` に依存して解釈される。  
domain層では `RawTx::{Eth2718, IcSynthetic}` に分離する。

### 3) tx_id 生成ルール（固定）

```
tx_id = keccak256(
  "ic-evm:storedtx:v2" || kind_u8 || raw ||
  caller_evm? || len(canister_id)||canister_id || len(caller_principal)||caller_principal
)
```

* `kind_u8`: **固定1byte**（0x01/0x02）
* principalは **length prefix(u16be) + bytes**
* `caller_evm` は `Some` の場合のみ 20 bytes を混ぜる

### 3.5) caller_evm の導出ルール（固定）

`caller_evm` は `@dfinity/ic-pub-key` の `chainFusionSignerEthAddressFor` と同一規則で導出する。

1. `ic-pub-key` の `derive_ecdsa_key` を signer canister id（`grghe-syaaa-aaaar-qabyq-cai`）と `key_1` で呼ぶ  
2. derivation path を `[0x01, principal_bytes]` にする  
3. 返却された派生公開鍵（compressed sec1）を uncompressed に展開する  
4. 派生公開鍵（uncompressed sec1 65 bytes）の先頭 `0x04` を除いた64 bytesを `keccak256`  
5. hash末尾20 bytesを EVMアドレスとする

* `principal_bytes = ic_cdk::caller().as_slice()`（**length prefix は付けない**）
* signer定数は canister id と key id をコード固定（`CHAIN_FUSION_SIGNER_CANISTER_ID` / `KEY_ID_KEY_1`）で運用する
* 導出APIは `Result<[u8;20], AddressDerivationError>` を返し、失敗時ゼロアドレスフォールバックは禁止

### 4) decode と drop / reject

* `Storable::from_bytes` は **trapしない**
* invalid/旧データは **StoredTxBytes::invalid** として復元
* service層で `StoredTx::try_from` を必ず通す  
  → 失敗時は **drop_code=decode**

### 5) fee 判定（rekey/ordering）

* rekey/ready 判定は **fee_fieldsのみ**を使用（decode不要）
* execute直前のみ decode（失敗時は drop_code=decode）

### 6) EthSigned との違い

* IcSynthetic は `caller_evm / canister_id / caller_principal` が必須
* EthSigned は raw が EIP-2718、生caller/principalは空

### 7) nonce 運用（IcSynthetic/共通）

**expected_nonce(sender)** を「次に受理されるnonce」と定義する。

* 参照元は **sender_expected_nonce（stable）** を優先し、未初期化なら **EVM state nonce** で初期化
* `submit_ic_tx` は **nonce == expected_nonce** のときのみ受理
* `nonce < expected_nonce` → Reject（NonceTooLow）
* `nonce > expected_nonce` → Reject（NonceGap）
* 同一nonceの置換は **fee↑のみ許可**（それ以外は Reject: NonceConflict）

**nonce 消費の区分**

* ExecFailed（実行に入った失敗）→ **nonce消費**
* Decode失敗など実行前不正 → **nonce非消費**
* nonce運用/置換ルールは **EthSigned も同一**

### 8) expected_nonce_by_address（運用query）

* `expected_nonce_by_address(address)` は **次に受理されるnonce** を返す
* 初期化済みなら **sender_expected_nonce（stable）**、未初期化なら **EVM state nonce**
* 返り値の用途は **送信前のnonce確認/スモーク** に限定する

### 9) EthSigned の tx hash（運用/UX）

* `tx_id` は内部IDとして保持
* EVM互換の利用に備えて **`eth_tx_hash = keccak(raw)` を別途保持/返却**するのが望ましい

senderごとのnonce待ち＋ready_queueに進化（ここで完成）

最終的に 3) まで行かないなら、手数料並べ替えは “見た目だけ” になりがち。

テスト（最低限）

同一 tx 集合・同一 seq → 常に同じ順序で inclusion される（決定性）

sender A の nonce 0/2 があっても nonce 2 は ready に上がらない（nonceゲート）

base_fee/min_tip を満たさない 1559 が確実に drop（コード一致）

queue_snapshot の cursor は **offset ではなく seq**（exclusive）で扱う。

legacy の gas_price < min は submit で reject

## 10) 追加テスト（運用に効く）

* legacy/1559 の `effective_gas_price` が receipt まで一致する
* 1559 の min fee 未満が submit で reject（drop ではない）
