# Solidity/Vyper Compatibility

## TL;DR
- EVM実行系の基本導線として `eth_call` / `eth_estimateGas` / `eth_sendRawTransaction` を利用できる。
- 互換の詳細は Gateway の実装・互換表を正本として扱う。

## 利用可能な主要メソッド
- `eth_call`
- `eth_estimateGas`
- `eth_sendRawTransaction`

## 参照先
- `tools/rpc-gateway/README.md`
- `crates/evm-core/src/tx_decode.rs`
