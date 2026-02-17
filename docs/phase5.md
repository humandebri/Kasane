# Phase5（OP後のプロダクト化フェーズ）

## 目的

* Phase4の安全性を壊さずに **実運用**へ持ち込む
* スループット/コスト/運用を現実にする
* EVM互換と開発者体験を引き上げる

## 非目的

* セキュリティモデルの変更（OP→ZK）
* Phase0/4のfreeze破壊（やるならハードフォーク）

---

## 1) パフォーマンス（State commitment 差分化）

* touched set を本格利用して差分更新
* もしくは StateCommitter差し替えで MPT互換化

成果:

* state_root 計算が **O(changes log N)** になる
* ブロック生成が現実速度になる

---

## 2) 過去state参照（スナップショット/履歴）

* チェックポイント（Nブロックごと）で snapshot
* 間は差分ログで復元
* eth_call の過去ブロック対応

---

## 3) RPC互換拡張（開発者が本当に使える）

優先順:

1. `eth_getLogs`（最初はブロック走査でOK）
2. `eth_feeHistory / eth_gasPrice`
3. `eth_subscribe`（後回し）
4. trace系（もっと後）

---

## 4) ブリッジ拡張（資産と接続の現実化）

* token allowlist + metadata registry
* 複数L1/L2対応（必要なら）
* L1↔L2メッセージパッシング

---

## 5) 分散運用（単一canisterの限界超え）

選択肢（現実順）:

* execution canister
* state canister（KV専用）
* rpc gateway canister

または、重い周辺だけ外に逃がす（ログindex/分析など）

---

## 6) “ICPから呼べる価値”のプロダクト化

* submit_ic_tx + auto-mine を中心に SDK/権限/課金/レート制限テンプレ
* “ワークフロー→EVM確定”のライブラリ化
* サンプルdappを複数用意

---

## Phase5は必要か？

Phase4だけでも「担保付き出金」は成立するが、

* 速度
* RPC互換
* 運用
* 接続（橋）

が弱いと開発者が定着しない。
**実用チェーンにするならPhase5はほぼ必須。**
