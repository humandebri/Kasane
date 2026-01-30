# Phase2.2（ICPブリッジの作成）

## 位置づけ（順序）

* **L2の基本機能が整ってから着手**する  
  （Phase2.1/2.3の後、またはPhase3手前が目安）

## 目的

* **ICP↔L2**の資産/メッセージ移動を最短で成立させる
* IC内部機能（管理API/リレー/クロスキャニスタ呼び出し）を活用して簡素に実現する

## 最低限の構成（Relayer不要）

* **ICP側**: BridgeVault canister（custody + deposit + finalizeWithdrawal）
* **L2側**: L2Bridge（finalizeDeposit + withdraw）
* **Relayer**: 原則不要（必要なら canister内の retry queue のみ）

## 仕様（必須）

* **冪等性**: depositId / withdrawId を固定
* **ガードレール**: pause / limit / allowlist
* **Trusted権限**: 初期は運用権限で開始（将来のガバナンス移行を想定）

### 冪等ID（固定）

* `depositId = hash(ledger_txid or block_index, token, amount, l2_recipient)`
* `withdrawId = hash(l2_txid, seq or index, token, amount, icp_recipient)`

### 失敗と再試行

* cross-canister が失敗しても **未完了状態**を残す
* retry で最終的に整合する（2重mint/2重払い防止）

### ICSynthetic の利用（簡素化ポイント）

* L2側の mint/burn を **ICSynthetic** で実行すると最短  
* 署名/ガス/送信を省略できる（ただし冪等性は必須）

## 合格条件

* ICP→L2 の入金が成立する
* L2→ICP の出金が成立する
* 二重実行が防止できる
* canister間失敗後でも retry で最終的に整合する
