# local wrap/unwrap smoke

目的: 今回の wrap / unwrap 修正が local で壊れていないことを、最短の手順で再確認する。

## 確認対象

- wrap:
  - `submit_wrap_request` が gas quote 段階で落ちない
  - その後の fee collection 段階まで進む
- unwrap:
  - `submit_ic_tx` 経由の unwrap request が dispatch される
  - 不正 vault は `DispatchFailed`
  - 正しい `wrap_canister_id` bytes は `Dispatched`
- query:
  - `rpc_eth_call_object`
  - `rpc_eth_estimate_gas_object`

実 ledger を含む local smoke は
[local_wrap_unwrap_ledger_smoke.ja.md](/Users/0xhude/Desktop/ICP/Kasane/docs/ops/local_wrap_unwrap_ledger_smoke.ja.md)
を使う。こちらは `WrapTokenFactory` deploy、successful mint、wrapped token balance、mint receipt `status = 1` まで確認する。

## 前提

- 実行ディレクトリはリポジトリルート
- 必要コマンド:
  - `cargo`
  - `icp`
  - `python`
- Rust target:
  - `wasm32-unknown-unknown`
- local managed network を使う場合:
  - `scripts/icp_local_clean_start.sh` が通ること
- PocketIC E2E を使う場合:
  - `crates/evm-rpc-e2e/pocket-ic` か `POCKET_IC_BIN` が利用可能であること

## 手順

### 1. local network を clean start

```bash
scripts/icp_local_clean_start.sh
```

期待:

- `icp network start local -d` が成功する
- status JSON が出る

### 2. canister wasm を build

```bash
cargo build --release --target wasm32-unknown-unknown -p ic-evm-gateway -p wrap-canister
```

期待:

- `target/wasm32-unknown-unknown/release/ic_evm_gateway.wasm`
- `target/wasm32-unknown-unknown/release/wrap_canister.wasm`

### 3. official ledger wasm を準備

```bash
bash scripts/prepare_ci_icrc1_ledger_wasm.sh && export ICP_LEDGER_WASM="$PWD/third_party/dfinity/ledger-suite-icrc-2026-03-09/ic-icrc1-ledger.wasm"
```

期待:

- `ICP_LEDGER_WASM` が official `ic-icrc1-ledger.wasm` を指す
- `wrap_unwrap_flow_e2e` が CI と同じ ledger wasm を使う

### 4. wrap / unwrap の PocketIC E2E

```bash
cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --test wrap_unwrap_flow_e2e -- --nocapture
```

期待:

- `wrap_submit_request_reaches_fee_collection_after_gateway_gas_quote ... ok`
- `unwrap_dispatch_succeeds_with_real_wrap_canister ... ok`

意味:

- wrap は `fee.quote_*` では失敗せず、dummy ledger による `fee.call_failed:*` まで進む
- unwrap は real `wrap_canister` 相手に `Dispatched` まで到達する

### 5. request_id 検証の回帰

```bash
cargo test -p ic-evm-gateway resolve_wrap_submit_ok -- --nocapture
```

期待:

- matching request_id は accept
- mismatched request_id は reject

補足:

- `request_id_mismatch` は real `wrap_canister` の local 正常系 smoke では人工的に作っていない
- local 実機では `Dispatched` 到達を確認し、不一致検知そのものはこの unit test を正とする
- real-ledger smoke で見る `wrap` の `Succeeded` は mint tx 受理の意味で、EVM inclusion 完了は別途 receipt で確認する

### 6. query 経路の wrap precompile 回帰

```bash
cargo test -p ic-evm-core --test wrap_precompile_query -- --nocapture
```

期待:

- `wrap_precompile_eth_call_object_succeeds_in_query_path ... ok`
- `wrap_precompile_eth_estimate_gas_succeeds_in_query_path ... ok`

### 7. 必要なら local query smoke

```bash
scripts/query_smoke.sh
```

期待:

- `rpc_eth_*` query が agent.query 経路で成功する

## 失敗時の切り分け

### `fee.quote_*` で失敗する

- `wrap_canister -> evm_canister` の gas price 取得経路を確認する
- `rpc_eth_gas_price` の戻り値 shape / decode を確認する
- gas price が未初期化なら、先に block 生成や seed tx を確認する

### `fee.call_failed:*` で止まる

- これは gas quote 通過後に fee ledger 側で止まっている
- dummy ledger を使う PocketIC E2E では想定内

### `DispatchFailed` / `wrap.arg.vault_not_allowed`

- unwrap compact payload 内の vault bytes が `wrap_canister_id` と一致していない
- local 実機確認では principal bytes をそのまま使う

### `auth.kasane_required`

- real `wrap_canister` は `submit_unwrap_request` caller を `kasane_canister` と照合する
- E2E では init args の `kasane_canister` を gateway 側 canister id に揃える

### local network が起動しない

- `icp network stop local`
- `scripts/icp_local_clean_start.sh`
- `pkill -f 'pocket-ic --ttl'`
- それでも不安定なら PocketIC E2E を優先する

## 終了

managed local network を止める場合:

```bash
icp network stop local
```
