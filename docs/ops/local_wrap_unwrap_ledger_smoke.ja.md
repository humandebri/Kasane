# local wrap/unwrap smoke with real ledger

目的: official ICRC ledger を local managed network に立て、`wrap` の fee pull / successful mint と `unwrap` の ledger transfer を実 ledger で確認する。

## 前提

- 実行ディレクトリは repo root
- 必要コマンド:
  - `cast`
  - `icp`
  - `dfx`
  - `cargo`
  - `curl`
  - `didc`
  - `forge`
  - `python`
  - `node`
  - `npm`
- Rust target:
  - `wasm32-unknown-unknown`
- `tools/wrapper-vite` の依存が未導入なら script が `npm ci` を実行する

## 実行

```bash
scripts/local_wrap_unwrap_ledger_smoke.sh
```

## script が行うこと

1. `icp network stop local` / `icp network start local -d` 相当で local managed network を clean start
2. 公式 ledger artifact を準備
   - レポ同梱の `third_party/dfinity/ledger-suite-icrc-2026-03-09/ic-icrc1-ledger.wasm`
   - `${LEDGER_CACHE_DIR}/<release>/ledger.did` に置く release 対応の `ledger.did`
3. local ledger canister を detached で作成し、ICRC-2 有効で install
4. `evm_canister` と `wrap_canister` を local install
   - `wrap_canister` の `allowed_assets` には local ledger canister を入れる
5. ledger に test caller と `wrap_canister` の初期残高を入れる
6. `submit_ic_tx` で `WrapTokenFactory` を deploy
   - この初回 tx により gas price も初期化される
7. allowance なしの `submit_wrap_request` で `insufficient_allowance` を確認
8. `icrc2_approve` 後に `submit_wrap_request` を再送し、worker が
   - `fee_ledger_tx_id != null`
   - `pull_ledger_tx_id != null`
   - `mint_tx_id != null`
   - `mint_failed_recoverable = false`
   になることを確認
9. mint tx receipt が `status = 1` になることと、
    - factory の `predictTokenAddress(bytes,uint8)`
    - factory の `getTokenAddress(bytes)`
    - wrapped token の `decimals()`
    - wrapped token の `balanceOf(address)`
    が期待どおりであることを確認
10. `submit_ic_tx` で unwrap request を起票し、
    - gateway 側 `Dispatched`
    - wrap 側 `Succeeded`
    - `ledger_tx_id != null`
    を確認
   - unwrap calldata は `tools/wrapper-vite` の helper が生成する compact payload を使う

## 期待結果

- wrap:
  - `fee.quote_*` では失敗しない
  - approve 前は `fee.transfer_from_failed:insufficient_allowance:*`
  - approve 後は fee pull と asset pull が成功する
  - `WrapRequestResult.status = Succeeded` は wrap canister が mint tx を gateway に受理させたことを表す
  - EVM inclusion 完了は別で mint receipt `status = 1` を確認する
  - factory deploy と initial mint が成功する
  - `predictTokenAddress(bytes,uint8)` と `getTokenAddress(bytes)` が一致する
  - wrapped token の `decimals()` が ledger metadata と一致する
  - wrapped token の `balanceOf` が `WRAP_AMOUNT` になる
- unwrap:
  - 正しい vault bytes で `Dispatched`
  - worker が `icrc1_transfer` を完了し、`ledger_tx_id` が保存される
  - unwrap 入力形式は旧 ABI ではなく compact payload 前提

## 主な環境変数

- `ICP_IDENTITY_NAME`
  - local で deploy / call に使う identity
- `LEDGER_RELEASE`
  - 既定値は `ledger-suite-icrc-2026-03-09`
  - レポ同梱 wasm と合わせるので、別 release を使う場合は `third_party/dfinity/<release>/ic-icrc1-ledger.wasm` も追加する
  - `latest` は受け付けない。固定 release を指定する
- `GENESIS_BALANCE_WEI`
  - 明示的に小さくしても script が factory deploy / wrap mint / unwrap submit の前払い上限を見て必要最小値まで自動補正する
- `LEDGER_CACHE_DIR`
  - ledger DID を release ごとのサブディレクトリに保存する
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
  - 例: `pkill -f 'pocket-ic --ttl'`
- `request_id_mismatch` は real `wrap_canister` に不正応答を返させないと local 実機で作りづらい。
  そのため、この smoke では `Dispatched` / `Succeeded` の正常系を確認し、不一致検知自体は
  `cargo test -p ic-evm-gateway resolve_wrap_submit_ok -- --nocapture`
  で担保する。
- query / update 呼び出しは Candid 引数の decode ずれを避けるため、script 内で `didc encode` した hex を使っている。
  local で手動確認を追加する場合も同じ形に揃えると切り分けしやすい。
