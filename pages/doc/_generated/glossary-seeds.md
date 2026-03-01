# Glossary Seeds（自動生成）

- `tx_id`
  - canister内部識別子（32 bytes）
- `eth_tx_hash`
  - Ethereum互換ハッシュ（`keccak256(raw_tx)`）
- `IcSynthetic`
  - Principal由来senderで投入する canister独自tx種別
- `EthSigned`
  - 署名済みEthereum raw tx
- `auto-production`
  - キュー投入後に timer 駆動で block生成する仕組み
- `RpcReceiptLookupView::PossiblyPruned`
  - prune境界により receipt の所在断定が難しい状態
- `Pruned`
  - prune済み範囲のため参照不可
- `expected_nonce_by_address`
  - sender nonce参照 query
- `rpc_eth_get_logs_paged`
  - 制限付きログ取得 API
- `base_fee_per_gas`
  - ブロック基準の基本手数料
- `max_fee_per_gas` / `max_priority_fee_per_gas`
  - EIP-1559 feeパラメータ
- `blockTag`
  - `latest` / `pending` / `safe` / `finalized` / QUANTITY
- `safe_stop_latched`
  - ops安全停止ラッチ状態
- `critical_corrupt`
  - 重大破損フラグ

## 根拠
- `README.md`
- `crates/ic-evm-wrapper/evm_canister.did`
- `crates/ic-evm-rpc/src/lib.rs`
- `tools/rpc-gateway/src/handlers.ts`
