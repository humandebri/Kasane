# Phase4 Spec（OP / Fault Proof / L1担保）

## Phase4の到達点

* L1に **tx列（DA）** があり、誰でも再実行できる
* 出力（state_root等）に対し **第三者がchallenge可能**
* disputeが起きたら **L1上の1-step FPVM** で決着
* 出金は **finalized output のみ**を根拠にする

---

## 0) Phase4で凍結する仕様（Freeze）

### 0.1 OutputRootの定義

```
output_root = keccak256(domain || l2_block_hash || state_root || tx_list_hash || outbox_root)
```

* domain: 固定（衝突回避）
* outbox_root: 出金要求のMerkle根

### 0.2 Batch（DAデータ）のエンコード

```
batch = concat( u32_be(len) || tx_bytes )*
batch_hash = keccak256(batch)
```

* tx_bytes: Phase0/1で凍結済み（Eth raw / ICSynthetic）

### 0.3 再実行環境（Fixed Env）

* timestamp = parent + 1 などの決定性ルールを固定
* ここを破るとFPVM一致が崩れる

---

## 1) Phase4.1 DA（Data Availability）

### 1.1 L1: BatchInbox

* `appendBatch(bytes batch, bytes32 batchHash, uint64 startBlock, uint64 endBlock)`
* event: `BatchAppended(batchHash, startBlock, endBlock)`

### 1.2 L2: batch_hash を BlockData に記録

* `block.batch_hash` を保存
* relayer生成用に `get_batch_bytes(start,end)` を用意

### 1.3 OutputOracle拡張

* `proposeOutput(l2BlockNumber, output_root, batch_hash)`

**合格条件**

* L1の batch だけで第三者が再実行入力を得られる

---

## 2) Phase4.2 Outbox（出金の証明可能化）

### 2.1 L2: Outbox Merkle

* withdrawal leaf を集めて outbox_root を算出

```
leaf = keccak256(domain || l1Token || toL1 || amount || l2TxId || index)
```

### 2.2 L1: proof-based withdraw

* `proveWithdrawal(output, leaf, proof)` が通ったら `finalizeWithdrawal`

**合格条件**

* L2→L1出金が「運営承認」ではなく **証明**で通る

---

## 3) Phase4.3 OP（Fault Proof）

### 3.1 FPVMの選定

* **Cannon系（MIPS風）推奨**
* RISC-Vも可だが実装量が増える

### 3.2 再実行プログラム

入力:

* pre_state_root
* batch（L1 BatchInbox）
* fixed_env_params

出力:

* post_state_root
* outbox_root

### 3.3 Preimage Oracle

* `loadPreimage(hash, bytes)` を L1に用意
* 必要時に提示（ガス最適化）

### 3.4 Dispute Game

* proposer が output_root 提案
* challenger が challenge（bond）
* 二分探索で1-stepに縮約
* FPVM stepper で勝敗決定

**合格条件**

* 不正 output を第三者が止められる
* dispute が L1上の1-stepで完結

---

## 4) Phase4.4 ブリッジの最終化

* 正規ルート: `proveWithdrawal(finalized_output, proof) -> finalize`
* trustedルート: 緊急用としてのみ残す（timelock必須）

---

## 5) 作業順序（依存関係）

1. DA（BatchInbox）
2. Outbox（proof-based withdraw）
3. FPVM（再実行 + 1-step）
4. Dispute Game（二分探索 + step）
5. Bridgeをfinalized outputに接続

---

## 6) Phase4の重さ（正直な見積もり）

* 再実行プログラムの作成は可能だが **FPVM向けに落とすのが重い**
* 1-step検証 + preimage は **大工事**

ただし「EVMの1-step」を作るよりは遥かに現実的。
