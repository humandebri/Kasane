# wrap-canister canfuzz

`dfinity/canister_fuzzing` の `canfuzz` を使って `wrap-canister` の `dispatch_unwrap_request` を fuzz するための独立 crate です。

## 目的

- candid decode で止まりにくい入力へ正規化して、`wrap-canister` の検証・重複判定・保存経路を継続的に探索する
- update が成功した入力では、`get_request` で同じ `request_id` を引けることまで確認する
- seed corpus は `fuzz/wrap-canister-canfuzz/corpus/` を使う。空でもディレクトリ自体は必要
- 実行時に `wrap_canister.wasm` を canfuzz 向けに instrument した一時 wasm を生成してから PocketIC に install する

## 実行

```bash
scripts/run_wrap_canister_fuzz.sh
```

スクリプトは既存の `crates/evm-rpc-e2e/pocket-ic` と `.canbench/pocket-ic` を自動検出し、見つからない場合だけ `POCKET_IC_BIN` の明示指定にフォールバックします。

必要に応じて wasm の場所を固定できます。

```bash
WRAP_CANISTER_WASM=/absolute/path/to/wrap_canister.wasm scripts/run_wrap_canister_fuzz.sh
```

クラッシュ再現は `WRAP_CANISTER_FUZZ_ONE_INPUT` に保存済み input を渡します。

```bash
WRAP_CANISTER_FUZZ_ONE_INPUT=/absolute/path/to/crash scripts/run_wrap_canister_fuzz.sh
```

補助スクリプトを使う場合:

```bash
scripts/replay_wrap_canister_fuzz_input.sh /absolute/path/to/input
```
