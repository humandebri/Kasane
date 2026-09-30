# 更新・復元の検証と mainnet 移行の残件（2026-09-30）

## 対象

main `ac5b5c6912a145a71aa35d014829af0e17e7edf2` を基準に、PocketIC の
更新・snapshot 復元テストと wrap 後の更新テストを追加した候補を検証する。
本番 canister の更新・停止・送金はこの作業では実行しない。

## 更新テストの範囲

- 取引を実行し、残高・nonce・receipt を記録して同じ候補 Wasm に更新する。
  記録した状態の保持と次の取引の成功を確認する。
- 更新前の snapshot を復元し、残高・nonce・receipt が snapshot 時点に戻り、
  その nonce から再取引できることを確認する。
- wrap で token を mint した後に更新し、token 残高・nonce・成功済み wrap request の
  保持を確認する。更新後に approve と unwrap を実行し、ledger の払い出しまで検査する。

初回 install と upgrade に同じ候補 Wasm を使う。旧稼働版からの schema 移行、
処理中の外部送金、全ての保存領域を網羅する検証ではない。
RPC E2E が読み込む artifact は postprocess 前の release Wasm。

## 検証結果

2026-09-30 のローカル検証結果:

- `cargo check --workspace`: 成功。
- `scripts/predeploy_smoke.sh`: release Wasm build 成功、RPC E2E 12 件成功・1 件スキップ。
  追加した更新・snapshot 復元テストも成功。
- `wrap_unwrap_flow_e2e`: 6 件成功。追加した wrap 後の更新・unwrap テストも成功。
- 変更した Rust テスト 2 ファイルの `rustfmt --check` と `git diff --check`: 成功。

以下のコマンドで再現する。

```sh
cargo check --workspace
scripts/predeploy_smoke.sh
CARGO_TARGET_DIR=target/e2e-lifecycle cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --locked \
  --test wrap_unwrap_flow_e2e -- --test-threads=1
```

PocketIC の実行にはホストに合う `POCKET_IC_BIN` が必要。
wrap E2E の `ICP_LEDGER_WASM` は `scripts/prepare_ci_icrc1_ledger_wasm.sh` が
選ぶ vendored official ledger を指定する。Foundry の build artifact も必要。
別 Cargo.lock の E2E 成果物と workspace 成果物は専用 `CARGO_TARGET_DIR` で隔離できる。

基準 main の EVM 証明は [PR #98](https://github.com/humandebri/Kasane/pull/98) に
含まれ、Lean 明示定理 36 件、Verus 198 verified / 0 errors、Rust/Lean 比較 439 ケース。
検証範囲と未証明部分は [EVM 検証結果](../../proofs/evm/verification-results.md) を参照。
今回の更新テストは新しい形式証明を追加しない。

## 稼働版からの移行に残る確認

2026-09-29 の既存調査記録では、canister `4c52m-aiaaa-aaaam-agwwa-cai` の
schema は 6、候補の `CURRENT_SCHEMA_VERSION` は 8。
稼働 module hash は `747ea3db560e601f3d2fcef43798cb695ce333493ffb2fceb500a0a5e2dde7af`。
これらは今回の候補で再取得した値ではなく、稼働 Wasm と commit の対応も未確定。

稼働版の artifact と代表的な schema 6 データを使い、隔離環境で schema 8 への
移行を検査する必要がある。旧構成の別 wrap canister に残る request と外部 ledger の
処理状況も、統合構成への移行前に照合する。

同日の記録には累積 `decode_failure_count=1`、ラベル `unwrap_request` がある。
この counter だけでは未処理 request や障害の解消を判定できないため、該当 request と
障害処置記録を照合する。

release endpoint guard は過去の検査で `composite_query` の分類不具合により失敗した。
最終 postprocess artifact の guard と、その artifact を使う E2E は今回の検証に含まれない。
候補同士の更新テストの成功から、稼働版の移行や最終 release artifact の合格を導かない。

## snapshot 復元

snapshot は Wasm・heap・stable memory を復元する。
復元後に reinstall を実行すると復元データが消えるため、
[復旧手順](../ops/chain-state-migration.md) からそのコマンドを削除する。
外部 ledger など別 canister の状態は巻き戻らない。再開前に外部処理を照合する。
根拠は [ICP snapshot 仕様](https://docs.internetcomputer.org/guides/canister-management/snapshots/)。
