# ICP-EVM ic-wasm Workflow

## どこで・何を・なぜ
- どこで: `ic-evm-wrapper` の build/deploy 導線
- 何を: `ic-wasm` による Wasm 後処理の標準手順
- なぜ: バイナリ最適化、Candid整合性保証、運用の再現性向上

## 標準ビルド後処理
共通スクリプト:
- `scripts/build_wasm_postprocess.sh`

実行内容（順序固定）:
1. `shrink`
2. `optimize O3`
3. `metadata candid:service`
4. `check-endpoints`

デフォルト入出力:
- input: `target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm`
- output: `target/wasm32-unknown-unknown/release/ic_evm_wrapper.final.wasm`

実行例:
```bash
scripts/build_wasm_postprocess.sh
```

## 既存導線への組み込み箇所
- `scripts/ci-local.sh`
- `scripts/playground_manual_deploy.sh`

両方とも最終成果物として `ic_evm_wrapper.final.wasm` を使用する。

## ci-local モード運用（2026-02）
`scripts/ci-local.sh` は責務分離のため、モードで実行範囲を切り替える。

- `CI_LOCAL_MODE=github`
  - `.github/workflows/ci.yml` 相当のチェックのみ実行する。
  - 失敗ログは `[phase=github]` で始まる。
- `CI_LOCAL_MODE=smoke`
  - ローカル統合スモーク（deploy/seed/indexer検証）だけ実行する。
  - 失敗ログは `[phase=smoke]` で始まる。
- `CI_LOCAL_MODE=all`（既定）
  - `github` → `smoke` の順で両方実行する。
- `CI_LOCAL_SKIP_TOOL_INSTALL=1`（任意）
  - `github` フェーズで `cargo-deny` / `cargo-audit` の自動インストールを行わない。
  - その場合、ツール未導入なら即失敗する。

使い分け:
- GitHub CI失敗の再現を優先する場合は `CI_LOCAL_MODE=github`。
- canister接続やindexer挙動のローカル検証は `CI_LOCAL_MODE=smoke`。

## WASIスタブ化 (`--stub-wasi`) について
- スクリプトは `ENABLE_STUB_WASI=1` を受け付ける。
- ただし `ic-wasm` 側が `--stub-wasi` をサポートしていない場合は失敗させる。
- 失敗時は `ic-wasm --version` を確認し、対応バージョンを利用する。

実行例:
```bash
ENABLE_STUB_WASI=1 scripts/build_wasm_postprocess.sh
```

## プロファイリング導線
プロファイリング用スクリプト:
- `scripts/profile_heap_trace.sh`

目的:
- 通常リリースとは分離して、計測専用Wasmを作ってデプロイする。

注意:
- 現在の `ic-wasm 0.9.10` では `--heap-trace` が利用できないため、スクリプトは機能検出して stable memory trace にフォールバックする。
- stable memory trace 利用時は `START_PAGE`（必要に応じて `PAGE_LIMIT`）を指定して、既存領域と競合しないようにする。

実行例（staging想定）:
```bash
NETWORK=staging \
CANISTER_ID=<staging_canister_id> \
START_PAGE=131072 \
scripts/profile_heap_trace.sh
```

デプロイを行わずに計測用wasmだけ作る場合:
```bash
START_PAGE=131072 SKIP_DEPLOY=1 scripts/profile_heap_trace.sh
```

## BLOCK_GAS_LIMIT 精密化手順（固定値運用）
方針:
- ランタイム可変APIは追加しない。
- 定数 `DEFAULT_BLOCK_GAS_LIMIT` を実測に基づいて更新する。

評価環境:
- 最終判断は staging canister で行う（local は候補絞り込み）。

評価指標:
- `produce_block` 成否
- 1ブロック処理時間
- cycles差分
- drop率

候補例:
- `2_000_000`
- `2_500_000`
- `3_000_000`
- `3_500_000`
- `4_000_000`

採用ルール:
- 同一ワークロードで複数回計測し、失敗ゼロの最大候補を基準にする。
- 安全余裕として 20% ヘッドルームを持たせる。

反映先:
- `crates/evm-db/src/chain_data/runtime_defaults.rs` の `DEFAULT_BLOCK_GAS_LIMIT`

回帰確認:
- `crates/evm-core/src/revm_exec.rs`（block header gas_limit）
- `crates/evm-core/src/chain.rs`（base_fee update計算）

## 検証コマンド
```bash
bash -n scripts/build_wasm_postprocess.sh
bash -n scripts/profile_heap_trace.sh
scripts/build_wasm_postprocess.sh target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm /tmp/ic_evm_wrapper.final.wasm
```

## worktree 初期化手順（vendor/revm）
背景:
- `vendor/revm` が欠落した worktree では `cargo test` が依存解決で失敗する。

推奨:
1. 新規 worktree に移動する。
2. `scripts/sync_vendor_revm.sh` を実行する（引数省略可）。
3. `cargo test -p evm-db -p ic-evm-core` で確認する。

実行例:
```bash
scripts/sync_vendor_revm.sh
scripts/sync_vendor_revm.sh /Users/you/path/to/main/repo
```

## 監視とベンチマーク追加（2026-02）
### Prometheus形式メトリクス
- canister query: `metrics_prometheus() -> (variant { Ok : text; Err : text })`
- 実装: `crates/ic-evm-wrapper/src/prometheus_metrics.rs`
- 代表メトリクス:
  - `ic_evm_cycles_balance`
  - `ic_evm_stable_memory_pages`
  - `ic_evm_heap_memory_pages`
  - `ic_evm_queue_len`
  - `ic_evm_total_submitted / included / dropped`

### canbench
- 依存: `canbench-rs`（optional, feature: `canbench-rs`）
- 設定ファイル: `canbench.yml`
- 追加ベンチ:
  - `submit_ic_tx_path`
  - `produce_block_path`
  - `decode_ic_synthetic_header_path`
  - `decode_eth_signature_path`
  - `decode_eth_unsupported_typed_reject_path`
  - `state_root_migration_tick_path`

実行例:
```bash
cargo install canbench
canbench
canbench --persist
```

回帰ゲート実行例:
```bash
scripts/run_canbench_guard.sh
```

閾値調整（任意）:
- `CANBENCH_MAX_REGRESSION_PCT`（既定: `2.0`）
- `CANBENCH_TARGET_IMPROVEMENT_PCT`（既定: `5.0`）
- `CANBENCH_TARGET_BENCHES`（カンマ区切り。指定時のみ対象ベンチに改善閾値を適用）
