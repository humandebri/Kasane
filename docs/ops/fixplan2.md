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

### 運用ルール（2026-02-05 追加）

- [x] 全 public `update` は anonymous caller を拒否する（エラー: `auth.anonymous_forbidden`）
- [x] `inspect_message` で anonymous `update` を early reject し、不要な処理コストを抑制
- [x] 各 `update` 本体でも同一エラーで拒否する（inspectをバイパスしても fail-closed）

## PR9: SIMD性能PR（最後に分離）

- [ ] wasm32向け `+simd128` 有効プロファイルを追加
- [ ] SIMD ON/OFF の両ビルド導線を維持
- [ ] correctness一致テストを先に実施
- [ ] ベンチ結果と改善点を記録

## 追加改善（2026-02-05）

- [x] ハッシュ実装を `tiny-keccak` 直接呼び出しから `alloy-primitives::keccak256` へ統一
- [x] `tx_loc` の `Storable` を bincode v2（fixed-int + big-endian）へ移行し、旧24byte形式の後方互換デコードを追加
- [x] `debug_print` 中心のログを `tracing` イベントへ移行し、wrapper で JSON 出力サブスクライバを初期化

## 安全強化（2026-02-05, Guard/Migration/Log）

- [x] `Storable::to_bytes` は `encode_guarded` 経由で `BOUND.max_size` 超過を即時検知（`storable.encode.bound_exceeded`）
- [x] oversize テストは可変長 `Storable` を対象にし、固定長（`CallerKey`/`TxLoc` など）は対象外とする（oversize が発生しないため）
- [x] decode失敗時ポリシーを型別に分離
  - [x] fail-closed: `tx`, `receipt`, `state_root_meta`, `tx_loc`, `block`, `tx_index`
  - [x] default継続: `metrics`, `prune_state`, `ops` など運用補助系
- [x] schema移行は tick前提（`SchemaMigrationPhase` + cursor）で one-shot依存を排除
- [x] ログフィルタは `LOG_FILTER`（canister env var）> stable override > `info` の優先順
- [x] ログwriterは再入時に `debug_print` へ直接フォールバックし、長文は `truncated=true` のJSONで安全出力
- [x] `tx_loc` の codec は `wincode` へ移行
- [x] wincode config は `with_preallocation_size_limit(BOUND.max_size)` を必須化（OOM/過大確保の抑止）
- [x] env var 読み取りは `exists -> value` と長さガードを共通化（`LOG_FILTER`）
- [x] fail-closed 時の外部応答を統一（`rpc.state_unavailable.corrupt_or_migrating`）
- [x] dual-store（新MemoryIdへ copy 後に active store 切替）を `tx_locs` で実装
- [x] active 切替は Verify 成功後のみ実行（失敗時は旧ストア参照）
- [x] from_version>=3 はコピーを省略し、Verify 条件を緩和
- [x] 旧decodeは移行例外として `tx_loc` に限定し、方針コメントを明記

## 北極星設計（trap排除・状態機械化・安全停止）

### 原則（拘束条件）
- `ic_cdk::trap` は「到達不能なプログラマバグ」専用にする
- 外部入力・EVM実行結果・デコード失敗・サイズ超過・容量超過は `Result::Err` で返す
- `produce_block` は `Err` を見て `Dropped(reason)` に遷移させ、前進を保証する

### Txライフサイクル（状態機械）
- `Queued -> Prepared (任意) -> Included | Dropped(reason)` を固定
- ブロックが 0 件でも `Dropped` は必ず永続化する（キュー修復）
- `Dropped` は本文削除 + 墓標 + インデックス掃除 + 観測用リングをワンセットで適用

### fail-closed の理想形
- `mark_decode_failure(..., true)` で `needs_migration` を sticky に設定
- 以降の write は `reject_write_reason` で `NeedsMigration` を返し安全停止
- 「そのメッセージ内の停止」は trap ではなく write ガードの連鎖で担保する

### auto-mine の理想形（永続スケジューラ）
- `mining_next_at / mining_failures / mining_backoff_ms / last_error_code` を stable に保持
- 成功: failures=0, 通常間隔
- drop-only/NoExecutableTx: failures += 1, 短いバックオフ
- その他 Err: failures += 1, 指数バックオフ + 上限
- NeedsMigration: auto-mine 停止（運用復旧モード）
- 失敗理由/連続回数/バックオフ値をメトリクスで可視化

### 段階移行のスコープ（P0/P1）
- 設計は全域に適用
- 実装はまず **EVM実行結果のサイズ超過** を Err 化し、`Dropped(reason)` に寄せる

### drop-only 永続化の要件（P0の補強）
- `staged_drops` は最低限 `{ tx_id, sender(Option), nonce(Option), drop_code }` を保持する  
  - `advance_sender_after_tx` と `pending_by_sender_nonce` の掃除には **nonce が必須**
- `apply_drops_only` は **キュー修復のみ**を行う  
  - 対象: `tx_store / tx_locs / ready_queue / pending_* / dropped_ring / metrics`  
  - **禁止**: `block / receipt / tx_index / trie_commit / head` の更新
- `included_tx_ids.is_empty()` 分岐に入る前に **receipt/to_bytes/store が走っていない**ことをコード確認項目にする
