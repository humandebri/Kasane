# Ethereum Differences

## TL;DR
- 互換対象は Ethereum JSON-RPC + EVM実行意味論（完全互換ではない）。
- tx type は Legacy/EIP-2930/EIP-1559 を受理、EIP-4844/EIP-7702 は拒否。
- pending/mempool を提供しない。
- 単一シーケンサ前提で reorg前提挙動は提供しない。

## できること / できないこと

### できること
- Eth signed tx の投入・実行・receipt参照
- `eth_call` / `eth_estimateGas`（制限付き）
- ブロック/tx/receipt/log の参照

### できないこと
- 4844 blob tx / 7702 authorization tx
- mempoolやfilter/WebSocket購読のEthereum同等運用

## トランザクション互換
- Supported
  - Legacy (RLP)
  - EIP-2930 (`tx_type=1`)
  - EIP-1559 (`tx_type=2`)
- Not supported
  - EIP-4844 (`type=0x03`)
  - EIP-7702 (`type=0x04`)

## feeモデル差分
- `base_fee` は保持され、`compute_next_base_fee` で更新される。
- `effective_gas_price` は `max_fee`, `max_priority_fee`, `base_fee` から計算。
- `eth_gasPrice` は `max(base_fee + max(推定priority,min_priority), min_gas_price)` を返す。

## finality/reorg差分
- 単一シーケンサ前提
- `auto-production` 後ブロックは reorg前提で扱わない
- `latest/pending/safe/finalized` の一部は head扱いの制約あり

## 代表エラー
- `DecodeError::UnsupportedType`（4844/7702）
- `DecodeError::WrongChainId`
- `DecodeError::LegacyChainIdMissing`

## 落とし穴
- Ethereum L1と同じ pending/finalityモデルを前提にする
- 4844/7702 tx を送って互換がある前提で実装する

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core/src/tx_decode.rs`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core/tests/phase1_eth_decode.rs`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core/src/base_fee.rs`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/evm-core/src/revm_exec.rs`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/README.md`
- `/Users/0xhude/Desktop/ICP/Kasane/README.md`
