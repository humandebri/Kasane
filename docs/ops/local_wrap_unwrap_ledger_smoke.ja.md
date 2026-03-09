# local wrap/unwrap smoke with real ledger

目的: official ICRC ledger を local managed network に立て、`wrap` の fee pull / withdraw と `unwrap` の ledger transfer を実 ledger で確認する。

## 前提

- 実行ディレクトリは repo root
- 必要コマンド:
  - `icp`
  - `dfx`
  - `cargo`
  - `curl`
  - `python`
  - `node`
  - `npm`
- Rust target:
  - `wasm32-unknown-unknown`
- `tools/wrapper` の依存が未導入なら script が `npm ci` を実行する

## 実行

```bash
scripts/local_wrap_unwrap_ledger_smoke.sh
```

## script が行うこと

1. `icp network stop local` / `icp network start local -d` 相当で local managed network を clean start
2. 公式 ledger artifact を取得
   - `ledger.did`
   - `ic-icrc1-ledger.wasm.gz`
3. local ledger canister を detached で作成し、ICRC-2 有効で install
4. `evm_canister` と `wrap_canister` を local install
5. ledger に test caller と `wrap_canister` の初期残高を入れる
6. `submit_ic_tx` を 1 本投げて gas price を初期化
7. allowance なしの `submit_wrap_request` で `insufficient_allowance` を確認
8. `icrc2_approve` 後に `submit_wrap_request` を再送し、worker が
   - `fee_ledger_tx_id != null`
   - `pull_ledger_tx_id != null`
   - `mint_failed_recoverable = true`
   になることを確認
9. `withdraw_failed_wrap` で `withdraw_ledger_tx_id != null` を確認
10. `submit_ic_tx` で unwrap request を起票し、
    - gateway 側 `Dispatched`
    - wrap 側 `Succeeded`
    - `ledger_tx_id != null`
    を確認

## 期待結果

- wrap:
  - `fee.quote_*` では失敗しない
  - approve 前は `fee.transfer_from_failed:insufficient_allowance:*`
  - approve 後は fee pull と asset pull が成功する
  - mint は意図的に失敗し、`withdraw_failed_wrap` で回収できる
- unwrap:
  - 正しい vault bytes で `Dispatched`
  - worker が `icrc1_transfer` を完了し、`ledger_tx_id` が保存される

## 主な環境変数

- `ICP_IDENTITY_NAME`
  - local で deploy / call に使う identity
- `LEDGER_RELEASE`
  - `latest` または GitHub release tag
- `LEDGER_CACHE_DIR`
  - ledger artifact の保存先
- `WRAP_AMOUNT`
- `UNWRAP_AMOUNT`
- `LEDGER_APPROVE_AMOUNT`
- `WAIT_RETRIES`
- `WAIT_SECONDS`
- `SKIP_BUILD=1`
  - 既存の release wasm を使う。作業ツリーの別差分で build が壊れているときだけ使う

## 補足

- この smoke は local managed network 用です。PocketIC E2E の代替ではなく補完です。
- query path の `rpc_eth_call_object` / `rpc_eth_estimate_gas_object` は既存の
  [local_wrap_unwrap_smoke.ja.md](/Users/0xhude/Desktop/ICP/Kasane/docs/ops/local_wrap_unwrap_smoke.ja.md)
  と
  `cargo test -p ic-evm-core --test wrap_precompile_query -- --nocapture`
  で継続確認します。
- `icp network stop local` の後でも `127.0.0.1:8000` が埋まっていると local start は失敗する。
  その場合は stale な `pocket-ic` / replica process を止めてから再実行する。
