# Phase3 Spec + 実装計画（L1 Anchor + Trusted Bridge）

## 目的

* L1へ **出力（state_root等）** を投稿し、監視・監査の基準点を作る
* L1↔L2（EVM canister）で **資産移動（まずERC20）** を実現
* **Trustedモデル**を明示し、運用可能なガードレールを入れる

## 非目的

* Fault Proof / ZK証明（=L1担保）
* 特定L2実装への互換追従
* P2P / mempool / eth_subscribe

---

## 0) 重要な前提（L2の本丸）

* L2は「チェーンを作る」から「チェーンを証明する」に変わる
* **決定性は生命線**（同一入力→同一state_root）
* 監査/テストの量はPhase2以前の数倍になる

---

## 1) 全体アーキテクチャ（Phase3時点）

### コンポーネント

* **EVM canister**: 既存チェーン本体（順序確定・実行・state_root）
* **Relayer（外部プロセス）**: L1イベント監視 + L2適用 + L1への反映
  * Phase3は外部relayerで十分

### L1 Contracts

* **OutputOracle**（アンカー）
* **L1BridgeVault**（入金ロック・出金解放）
* （任意）Timelock / AccessControl

### L2 Contracts（あなたのEVM上）

* **L2Bridge**（withdrawイベント）
* **WrappedERC20**（L1 tokenごとのラップ）

---

## 2) L1 Anchor（OutputOracle）

### 2.1 最小仕様

* `postOutput(uint256 l2BlockNumber, bytes32 l2BlockHash, bytes32 stateRoot, bytes32 txListHash)`
* event: `OutputPosted(l2BlockNumber, l2BlockHash, stateRoot, txListHash)`
* 権限: `PROPOSER_ROLE`（DAO/マルチシグ）

### 2.2 投稿ルール

* 投稿頻度: every N blocks（例: 10〜100） or 手動
* `l2BlockNumber` は単調増加（戻れない）
* 同一 `(l2BlockNumber, l2BlockHash)` の再投稿は拒否 or no-op

> これは「担保」ではなく **監視の掲示板**。
> ただしブリッジ監視・監査の足場になる。

---

## 3) Trusted Bridge（ERC20から開始）

### 3.1 用語

* **Deposit**（L1→L2）: L1でロック → L2でmint
* **Withdraw**（L2→L1）: L2でburn → L1で解放

### 3.2 L1BridgeVault

* `deposit(l1Token, toL2, amount)`
  * `transferFrom` でVaultにロック
  * event: `DepositInitiated(l1Token, fromL1, toL2, amount, depositId)`

* `finalizeWithdrawal(l1Token, toL1, amount, withdrawId)`
  * `RELAYER_ROLE`（trusted）
  * `withdrawId` 二重実行防止
  * event: `WithdrawalFinalized(...)`

**ID定義（凍結）**

* `depositId = keccak256(l1TxHash || logIndex)`
* `withdrawId = keccak256(l2TxId || withdrawalIndex)`

### 3.3 L2側（EVM上）

**Option A（推奨：イベント方式）**

* `L2Bridge.withdraw(l1Token, toL1, amount)` が `WithdrawalInitiated` を emit
* relayer が **L2ログを走査**して L1 finalize

**Option B（簡易）**

* canisterが outbox を持つ（ログ不要）
* relayer は `eth_call` or canister query で読み取る

Phase3の推奨は **Option A**（ログ保存だけ追加）。

---

## 4) Phase3で追加する L2 側機能（canister内部）

### 4.1 Receiptに logs を保存（最低限）

* `ReceiptLike.logs: Vec<LogEntry>`
* `address(20) / topics(Vec<32>) / data(bytes)`
* **インデックスは作らない**（沼回避）

Relayer向けの簡易API：

* `get_logs(from_block, to_block, address?, topics?) -> Vec<LogEntryWithContext>`
* 実装は **ブロック範囲走査**のみ（十分）

### 4.2 Bridge Inbox/Outbox（stable）

* `processed_deposits: Set<depositId>`
* `processed_withdrawals: Set<withdrawId>`

### 4.3 L1→L2 deposit 適用API（update）

* `apply_deposit(DepositMessage) -> Result`
  * `DepositMessage{depositId,l1Token,fromL1,toL2,amount}`
* 未処理なら L2で mint（system-from）
* `processed_deposits.insert(depositId)`（冪等）

### 4.4 L2→L1 withdraw 検出

* relayer が `get_logs` で `WithdrawalInitiated` を検出
* withdrawId を計算して L1 finalize

---

## 5) セキュリティモデル（Phase3の現実）

### 5.1 Trustedの定義

* **RELAYER_ROLE** が出金を最終的に許可できる
* L1担保ではなく **DAO運用担保**

### 5.2 ガードレール（必須）

L1側:

* `pause()`（deposit/withdraw停止）
* `withdrawal_daily_limit`
* `token_allowlist`
* `timelock`
* `emergency_withdraw`

L2側:

* apply_deposit のレート制限
* processed_* による冪等性

---

## 6) データ保持（pruning）と履歴保存の方針

**結論（推奨）**

* L2は **pruning する**
* 履歴は **インデクサに外出し**する

理由:

* canister内の無期限保持はコスト/容量で破綻する
* 監査/分析/Explorerは外部indexerが現実的

最小要件:

* prune 前に **relayer/indexerが吸い出す仕組み**を用意
* OutputOracle投稿と整合する範囲は必ず保存

---

## 7) 実装タスク分解（Phase3チケット）

### 7.1 L1コントラクト

* OutputOracle（postOutput + role + event）
* L1BridgeVault（deposit / finalizeWithdrawal + pause + limits）
* Timelock/AccessControl

### 7.2 L2コントラクト

* WrappedERC20（mint/burn、bridgeのみmint可）
* L2Bridge（withdrawイベント emit）

### 7.3 EVM canister

* receipts に logs 保存
* `get_logs` query
* `processed_deposits / processed_withdrawals`（stable）
* `apply_deposit` update

### 7.4 Relayer

* L1: DepositInitiated → apply_deposit
* L2: WithdrawalInitiated → finalizeWithdrawal
* OutputOracle への定期投稿
