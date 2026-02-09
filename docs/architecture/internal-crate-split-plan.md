# どこで・何を・なぜ
- どこで: `ic-evm-wrapper` 周辺の内部構成
- 何を: 将来の内部crate分割の境界と移行順を定義する
- なぜ: 依存削減・命令数最適化・責務分離を安全に進めるため

## 目的
- `revm` を維持しながら、canister本体 (`ic-evm-wrapper`) の責務集中を解消する。
- 公開APIは変えず、内部実装のみ段階的に分割する。
- 分割後も canbench と既存テストで回帰検知できる状態を維持する。

## 先行実装（完了）
- `ic-evm-tx` を新設し、`tx_recovery`（Eth署名復元境界）を `ic-evm-core` から移管した。
- `ic-evm-core` は `ic-evm-tx` へ依存し、`alloy-consensus` への直接依存を解消した。
- 目的: `k256/alloy-consensus` の影響範囲を tx 専用crate に封じ込め、段階分割の起点を作る。

## 分割対象（将来）
1. `ic-evm-rpc-types`
- 内容: `*View`, `*Error`, DTO群、RPC用の軽量型
- 依存: `candid`, `serde` のみを原則にする
- 非目標: chain実行ロジックを入れない

2. `ic-evm-rpc`
- 内容: `rpc_eth_*` の入力検証・変換・参照導線
- 依存: `ic-evm-rpc-types`, `evm-core`, `evm-db`
- 非目標: cycle監視や運用モード制御を入れない

3. `ic-evm-ops`
- 内容: cycle監視、critical判定、write reject判定、運用パラメータ制御
- 依存: `evm-db`, `ic-cdk`
- 非目標: RPCレスポンス組み立てを入れない

4. `ic-evm-metrics`
- 内容: Prometheus出力整形、メトリクススナップショット変換
- 依存: `ic-metrics-encoder`, `ic-evm-rpc-types`
- 非目標: chain状態更新を入れない

## 境界ルール
- `ic-evm-wrapper` は canister entrypoint とオーケストレーションだけを担当する。
- crate間は「型 -> ロジック」の一方向依存を守る。
- `ic-evm-rpc-types` は最下層の共有型crateとして循環依存を禁止する。

## 移行順（実施フェーズ）
1. 型分離
- `*View` / `*Error` を `ic-evm-rpc-types` に移す。
- 既存APIシグネチャは不変とし、re-exportで呼び出し側互換を維持する。

2. メトリクス分離
- `prometheus_metrics.rs` を `ic-evm-metrics` に移し、snapshot型を型crate経由に統一する。

3. ops分離
- `observe_cycles`, `reject_write_reason`, ops状態読み書きを `ic-evm-ops` に移す。

4. RPC分離
- `rpc_eth_*` 群を `ic-evm-rpc` に移し、entrypointからは薄い委譲だけにする。

## 受け入れ条件
- 公開Candid/APIシグネチャに変更がないこと。
- `cargo test -p ic-evm-core -p ic-evm-wrapper --locked` が通ること。
- canbenchの主要ベンチで命令数が悪化しないこと（許容悪化は +2% 以内）。

## リスクと対策
- リスク: crate境界の循環依存
- 対策: 共有型を `ic-evm-rpc-types` に限定し、境界レビューを必須化する。

- リスク: 分割時に import/型名変更で差分が肥大化
- 対策: 1ステップ1責務で移行し、re-exportで段階互換を維持する。
