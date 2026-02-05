# Ethereum互換固定 実装チェックリスト（PR0〜PR9）

## 実装順（推奨）

1. PR0（差分テスト基盤）
2. PR1/PR2（Tx型整理・decode委譲）
3. PR3（実行フローの標準EVM化）
4. PR5（state root標準化）
5. PR4（base fee標準化）
6. PR6（receipt/log型寄せ）
7. PR7/PR8（エラー分類・署名責務分離）
8. PR9（SIMD性能PR）

## PR0: 差分テスト基盤（壊さない土台）

- [x] `tx -> receipt/log/state_root/gas/halt_reason` を固定化するスナップショットテストを追加
- [x] `block_hash/state_root/tx_list_hash` のブロック単位スナップショットを追加
- [ ] 参照実装との差分比較（differential test）をオフチェーン実行可能にする（reth突合は未接続）
- [x] 以降PRで「意図した差分」を判定できるCI導線を追加

## PR1: Tx表現の統一（TxIn導入）

- [x] `Eth/IcSynthetic` の統一入口型（`TxIn`）を導入
- [x] `StoredTx/RawTx/FeeFields` を再設計し、独自型乱立を削減
- [x] submit/executeの入口を `TxIn` に統一
- [x] 永続化形式はversioned encoding前提で更新

## PR2: decodeをライブラリ委譲

- [x] Eth decodeをalloy系API中心に寄せる（legacy/2930/1559/4844/7702）
- [x] 自前のRLP境界処理・type判定を削減
- [x] 失敗理由（invalid rlp/bad sig/wrong chain id等）を一貫分類にする

## PR3: 実行フローを標準EVMへ

- [x] 実行コンテキストを標準EVM設定へ統一
- [x] DBコミットは差分（Bundle/State差分）反映に整理
- [x] `effective_gas_price` はL2ガス価格の意味を維持
- [x] `total_fee = gas_used * effective_gas_price` を固定（追加会計なし）

## PR4: base fee標準化

- [x] `compute_next_base_fee` をalloy EIP-1559計算へ置換
- [x] 自前定数依存を削減し参照元準拠に統一
- [x] `phase1_fee_rules` へベースフィー遷移の参照実装一致テストを追加

## PR5: state root標準化（最優先）

- [x] 独自 `leaf_hash` / `compute_state_root_from_changes` 依存を廃止
- [x] 標準trie実装（`alloy-trie`）へ置換
- [x] iteration順序・エンコード差異によるrootずれをテストで固定

## PR6: receipt/log型を標準へ

- [x] `ReceiptLike/LogEntry` をalloy型準拠に整理（保存層のみ独自）
- [x] RPC変換コードを薄くする
- [x] export/indexer互換の確認・必要修正
- [x] Candid型を更新

## PR7: エラー/停止理由の固定

- [x] 文字列エラー中心の返却をやめ、安定分類へ寄せる
- [x] 実行エラーを上位へ伝播可能にする
- [x] canister APIへのマッピング表を固定

## PR8: 署名検証の責務分離

- [ ] Ingress側検証（Eth/IcSynthetic）とEVM内部precompile責務を分離
- [ ] `ecrecover` 等の二重実装を排除
- [ ] 境界仕様（どこで何を検証するか）を文書化

## PR9: SIMD性能PR（最後に分離）

- [ ] wasm32向け `+simd128` 有効プロファイルを追加
- [ ] SIMD ON/OFF の両ビルド導線を維持
- [ ] correctness一致テストを先に実施
- [ ] ベンチ結果と改善点を記録
