# Solidity/Vyper Compatibility

## TL;DR
- EVM実行は可能だが、JSON-RPC周辺は制限付き互換。
- deploy/callは可能だが、本リポ内でsolc/forge一発E2E手順は `要確認`。

## できること
- `eth_call`, `eth_estimateGas`, `eth_sendRawTransaction`

## 要確認
- Solidity/Vyperバージョンごとの互換マトリクス（一次情報不足）

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core/src/tx_decode.rs`
