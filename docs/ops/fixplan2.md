# op-revm移行 実装チェックリスト（PR0〜PR9）

## 実装順（推奨）

1. PR0（差分テスト基盤）
2. PR1/PR2（Tx型整理・decode委譲）
3. PR3（op-revm実行フロー移行）
4. PR5（state root標準化）
5. PR4（base fee標準化）
6. PR6（receipt/log型寄せ）
7. PR7/PR8（エラー分類・署名責務分離）
8. PR9（SIMD性能PR）

## PR0: 差分テスト基盤（壊さない土台）

- [x] `tx -> receipt/log/state_root/gas/halt_reason` を固定化するスナップショットテストを追加
- [x] `block_hash/state_root/tx_list_hash` のブロック単位スナップショットを追加
- [ ] 参照実装との差分比較（differential test）をオフチェーン実行可能にする（比較スクリプト/参照ファイルは追加済み、op-geth/reth突合は未接続）
- [x] 以降PRで「意図した差分」を判定できるCI導線を追加

対象ファイル（主）:
- `crates/evm-core/tests/*`
- `crates/evm-rpc-e2e/tests/rpc_compat_e2e.rs`
- `scripts/ci-local.sh`

## PR1: Tx表現の統一（TxIn導入）

- [x] `Eth/OpDeposit/IcSynthetic` の統一入口型（例: `TxIn`）を導入
- [ ] `StoredTx/RawTx/FeeFields` を再設計し、独自型乱立を削減
- [x] submit/executeの入口を `TxIn` に統一
- [ ] 永続化形式はversioned encoding前提で更新

対象ファイル（主）:
- `crates/evm-db/src/chain_data/tx.rs`
- `crates/evm-core/src/chain.rs`
- `crates/ic-evm-wrapper/src/lib.rs`

## PR2: decodeをライブラリ委譲

- [ ] Eth decodeをalloy系API中心に寄せる（legacy/2930/1559/4844/7702）
- [ ] Deposit decode/検証はop-revmロジックへ寄せる（source hash等）
- [ ] 自前のRLP境界処理・type判定を削減
- [ ] 失敗理由（invalid rlp/bad sig/wrong chain id等）を一貫分類にする

対象ファイル（主）:
- `crates/evm-core/src/tx_decode.rs`
- `crates/evm-core/tests/phase1_eth_decode.rs`
- `crates/evm-core/tests/phase1_ic_decode.rs`

## PR3: 実行フローをop-revmへ

- [ ] `MainnetContext` 実行を `OpContext + OpBuilder` ベースへ移行
- [ ] L1BlockInfo system tx をブロック先頭に注入
- [ ] L1 data fee徴収をhandler準拠で適用
- [ ] deposit/system txの特例処理とhalt理由（FailedDeposit等）を反映
- [ ] DBコミットは差分（Bundle/State差分）反映に整理

対象ファイル（主）:
- `crates/evm-core/src/revm_exec.rs`
- `crates/evm-core/src/chain.rs`
- `crates/evm-core/src/revm_db.rs`
- `crates/evm-core/Cargo.toml`

## PR5: state root標準化（最優先）

- [ ] 独自 `leaf_hash` / `compute_state_root_from_changes` 依存を廃止
- [ ] 標準trie実装（`alloy-trie` or `reth-trie`）へ置換
- [ ] iteration順序・エンコード差異によるrootずれをテストで固定
- [ ] 参照実装一致テストを追加

対象ファイル（主）:
- `crates/evm-core/src/state_root.rs`
- `crates/evm-core/src/chain.rs`
- `crates/evm-core/tests/phase1_hash.rs`

## PR4: base fee標準化

- [ ] 独自 `compute_next_base_fee` をalloy EIP-1559計算へ置換
- [ ] 自前定数依存を削減し参照元準拠に統一
- [ ] ベースフィー遷移の参照実装一致テストを追加

対象ファイル（主）:
- `crates/evm-core/src/base_fee.rs`
- `crates/evm-core/src/chain.rs`
- `crates/evm-core/tests/phase1_fee_rules.rs`

## PR6: receipt/log型を標準へ

- [ ] `ReceiptLike/LogEntry` をalloy型準拠に整理（保存層のみ独自）
- [ ] RPC変換コードを薄くする
- [ ] export/indexer互換の確認・必要修正
- [ ] Candid型を更新

対象ファイル（主）:
- `crates/evm-db/src/chain_data/receipt.rs`
- `crates/ic-evm-wrapper/src/lib.rs`
- `crates/ic-evm-wrapper/evm_canister.did`
- `tools/indexer/src/worker.ts`

## PR7: エラー/停止理由の固定

- [ ] 文字列エラー中心の返却をやめ、安定分類へ寄せる
- [ ] `OpHaltReason` / `OpTransactionError` を上位へ伝播可能にする
- [ ] Deposit failure特例を保持
- [ ] canister APIへのマッピング表を固定

対象ファイル（主）:
- `crates/evm-core/src/revm_exec.rs`
- `crates/evm-core/src/chain.rs`
- `crates/ic-evm-wrapper/src/lib.rs`

## PR8: 署名検証の責務分離

- [ ] Ingress側検証（Eth/IcSynthetic）とEVM内部precompile責務を分離
- [ ] `ecrecover` 等の二重実装を排除
- [ ] 境界仕様（どこで何を検証するか）を文書化

対象ファイル（主）:
- `crates/ic-evm-wrapper/src/lib.rs`
- `crates/evm-core/src/tx_decode.rs`

## PR9: SIMD性能PR（最後に分離）

- [ ] wasm32向け `+simd128` 有効プロファイルを追加
- [ ] SIMD ON/OFF の両ビルド導線を維持
- [ ] correctness一致テストを先に実施
- [ ] ベンチ結果と改善点を記録

対象ファイル（主）:
- `.cargo/config.toml`（新規）
- `scripts/ci-local.sh`
- `scripts/local_indexer_smoke.sh`
- `scripts/playground_manual_deploy.sh`

## 全PR共通ガード（必須）

- [ ] 既存 `MemoryId` は変更しない（必要時は追加のみ）
- [ ] `Storable` バイナリ変更は versioned encoding に限定
- [ ] upgrade前後互換テストを更新
- [ ] Candid/RPC互換テストを更新

対象ファイル（主）:
- `crates/evm-db/src/memory.rs`
- `crates/evm-db/src/chain_data/*.rs`
- `crates/evm-db/tests/phase0_upgrade.rs`
- `crates/evm-rpc-e2e/tests/rpc_compat_e2e.rs`
