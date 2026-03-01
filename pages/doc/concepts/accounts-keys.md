# Accounts & Keys

## TL;DR
- senderは `EthSigned` と `IcSynthetic` で導出経路が異なる。
- `expected_nonce_by_address` は20 bytes address前提。
- `IcSynthetic` は `from` をpayloadで受け取らず、caller情報から決定される。

## できること
- Eth signed txの署名senderを使う
- Principal由来sender（IcSynthetic）を使う

## できないこと
- bytes32風の値を20 bytes addressとして扱う

## `EthSigned` と `IcSynthetic` の違い
- `EthSigned`
  - 署名済みraw txから sender を復元
  - chain id検証を伴う
- `IcSynthetic`
  - wrapperが `msg_caller()` と `canister_self()` を付与して `TxIn::IcSynthetic` として投入
  - payloadに `from` を持たない
  - sender導出失敗は `AddressDerivationFailed` 系エラーになる

## `IcSynthetic` で必ず押さえる点
- `submit_ic_tx` の入力は Candid `record`（`to/value/gas_limit/nonce/max_fee_per_gas/max_priority_fee_per_gas/data`）
- 内部保存時の canonical bytes は `to_flag(0/1)` 形式
- nonce参照は `expected_nonce_by_address` を使う
- 戻り値は `eth_tx_hash` ではなく `tx_id`
- 実行確定は submit時点ではなく、後続blockで receipt 参照して判断する

## 安全な使い方
1. callerに対応する20 bytesアドレスを確定する
2. `expected_nonce_by_address` で nonce を取る
3. `submit_ic_tx` を送る
4. `get_pending(tx_id)` / `get_receipt(tx_id)` で追跡する

## 落とし穴
- Principalエンコード値をそのままaddressとして投入する
- `expected_nonce_by_address` に20 bytes以外を渡す
- `tx_id` を `eth_tx_hash` と同一視する
- submit成功を実行成功と誤認する

## 根拠
- `crates/ic-evm-wrapper/src/lib.rs`（`expected_nonce_by_address`）
- `crates/evm-core/src/tx_decode.rs`（`IcSynthetic` / `EthSigned`）
- `crates/evm-core/src/chain.rs`（`TxIn::IcSynthetic`）
- `README.md`
