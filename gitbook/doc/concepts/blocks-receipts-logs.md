# Blocks, Receipts, Logs

## TL;DR
- block/receipt/log は Candid型として公開される。
- receipt には `status`, `gas_used`, `effective_gas_price`, `contract_address`, `logs` を含む。

## ポイント
- `logs[].logIndex` はブロック内通番
- receipt lookup は `Found/NotFound/PossiblyPruned/Pruned`

## 落とし穴
- pruned履歴を永続参照できる前提で設計する
- `tx_id` と `eth_tx_hash` の照会経路を混在させる

## 根拠
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-wrapper/evm_canister.did`
- `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-rpc/src/lib.rs`
- `/Users/0xhude/Desktop/ICP/Kasane/tools/rpc-gateway/src/handlers.ts`
