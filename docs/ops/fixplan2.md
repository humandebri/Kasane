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
- [x] `StoredTx/RawTx/FeeFields` を再設計し、独自型乱立を削減
- [x] submit/executeの入口を `TxIn` に統一
- [x] 永続化形式はversioned encoding前提で更新

対象ファイル（主）:
- `crates/evm-db/src/chain_data/tx.rs`
- `crates/evm-core/src/chain.rs`
- `crates/ic-evm-wrapper/src/lib.rs`

## PR2: decodeをライブラリ委譲

- [x] Eth decodeをalloy系API中心に寄せる（legacy/2930/1559/4844/7702）
- [x] Deposit decode/検証はop-revmロジックへ寄せる（source hash等）
- [x] 自前のRLP境界処理・type判定を削減
- [x] 失敗理由（invalid rlp/bad sig/wrong chain id等）を一貫分類にする

対象ファイル（主）:
- `crates/evm-core/src/tx_decode.rs`
- `crates/evm-core/tests/phase1_eth_decode.rs`
- `crates/evm-core/tests/phase1_ic_decode.rs`

## PR3: 実行フローをop-revmへ

- [x] `MainnetContext` 実行を `OpContext + OpBuilder` ベースへ移行
- [x] L1BlockInfo system tx をブロック先頭に注入（内部実行のみ）
- [x] L1 data fee徴収をhandler準拠で適用
- [x] deposit/system txの特例処理とhalt理由（FailedDeposit等）を反映
- [x] DBコミットは差分（Bundle/State差分）反映に整理
- [x] `effective_gas_price` はL2ガス価格の意味を維持（L1/operator feeは別会計）
- [x] `total_fee = gas_used * effective_gas_price + l1_data_fee + operator_fee` を固定
- [x] L1 snapshotはブロック開始時に1回キャプチャし、同ブロック中の更新は次ブロック反映
- [x] `set_l1_block_info_snapshot` は `is_producing=true` 中に拒否
- [x] `revm_exec` は `BlockExecContext` 経由のみで実行し、stable readを行わない
- [x] system tx は内部専用・非計上（receipt/index未保存、ユーザー向けgas/fee集計に不算入）
- [x] L1BlockInfo calldata は spec 分岐（pre-ECOTONE / post-ECOTONE）で構築
- [x] `snapshot.enabled=false` のとき system tx 注入をスキップ
- [x] system tx の storage 更新をテストで検証
- [x] 既存データ互換・移行はPR3で扱わない（新運用データ前提）

対象ファイル（主）:
- `crates/evm-core/src/revm_exec.rs`
- `crates/evm-core/src/chain.rs`
- `crates/evm-core/src/revm_db.rs`
- `crates/evm-core/Cargo.toml`



## PR4: base fee標準化

- [x] **必須（PR4本体）**
- [x] `compute_next_base_fee` を alloy EIP-1559計算へ置換
- [x] 自前定数依存を削減し参照元準拠に統一
- [x] `phase1_fee_rules` へベースフィー遷移の参照実装一致テストを追加
- [x] **同梱ガード（推奨）**
- [x] `pr0_snapshots` 差分理由（system tx skip由来）をドキュメント化
- [x] system tx 不算入（receipt/index/gas/fee/tx件数）を共通アサートで固定
- [x] `spec_id` 分岐漏れ防止テスト（境界 + 全match）を追加
- [x] disabledスキップ時の観測性（メトリクス優先、ログ抑制）を維持
- [x] **受け入れ条件（PR4 Done）**
- [x] base fee遷移が参照実装と一致
- [x] `pr0_snapshots` の差分が「意図差分」として説明可能
- [x] system txがユーザー会計へ混入しないことをテストで保証
- [x] `spec_id` 境界（pre/post）および未対応追加時の検知導線がある
- [x] disabledスキップを観測できる

対象ファイル（主）:
- `crates/evm-core/src/base_fee.rs`
- `crates/evm-core/src/chain.rs`
- `crates/evm-core/tests/phase1_fee_rules.rs`


## PR5: state root標準化（最優先）

- [x] 独自 `leaf_hash` / `compute_state_root_from_changes` 依存を廃止
- [x] 標準trie実装（`alloy-trie`）へ置換
- [x] iteration順序・エンコード差異によるrootずれをテストで固定
- [x] 参照実装一致テストを追加（`phase1_hash` + `pr0_snapshots`意図差分固定）

対象ファイル（主）:
- `crates/evm-core/src/state_root.rs`
- `crates/evm-core/src/chain.rs`
- `crates/evm-core/tests/phase1_hash.rs`

## PR6: receipt/log型を標準へ

- [x] `ReceiptLike/LogEntry` をalloy型準拠に整理（保存層のみ独自）
- [x] RPC変換コードを薄くする
- [x] export/indexer互換の確認・必要修正
- [x] Candid型を更新
- [x] 固定仕様を明文化
  - topics順序保持・data生バイト保持
  - topics>4 decode reject（truncateしない）
  - statusは0/1固定
  - contract_addressはCREATE時のみSome
  - logs_bloomはPR6では非保存/非公開
  - `rpc_compat_e2e` 前に wrapper wasm を再ビルドしてから実行（`scripts/run_rpc_compat_e2e.sh`）

対象ファイル（主）:
- `crates/evm-db/src/chain_data/receipt.rs`
- `crates/ic-evm-wrapper/src/lib.rs`
- `crates/ic-evm-wrapper/evm_canister.did`
- `tools/indexer/src/worker.ts`

## PR7: エラー/停止理由の固定

- [x] 文字列エラー中心の返却をやめ、安定分類へ寄せる
- [x] `OpHaltReason` / `OpTransactionError` を上位へ伝播可能にする
- [x] Deposit failure特例を保持
- [x] canister APIへのマッピング表を固定

固定マッピング（wrapper `Rejected("exec.*")`）:
- `Revert` -> `exec.revert`
- `Decode(_)` -> `exec.decode.failed`
- `TxError(TxBuildFailed)` -> `exec.tx.build_failed`
- `TxError(TxRejectedByPolicy)` -> `exec.tx.rejected_by_policy`
- `TxError(TxPrecheckFailed)` -> `exec.tx.precheck_failed`
- `TxError(TxExecutionFailed)` -> `exec.tx.execution_failed`
- `FailedDeposit` -> `exec.deposit.failed`
- `SystemTxRejected` -> `exec.system_tx.rejected`
- `SystemTxBackoff` -> `exec.system_tx.backoff`（抑止通知。実失敗とは別扱い）
- `InvalidL1SpecId(_)` -> `exec.l1_spec.invalid`
- `InvalidGasFee` -> `exec.gas_fee.invalid`
- `EvmHalt(OutOfGas)` -> `exec.halt.out_of_gas`
- `EvmHalt(InvalidOpcode)` -> `exec.halt.invalid_opcode`
- `EvmHalt(StackOverflow)` -> `exec.halt.stack_overflow`
- `EvmHalt(StackUnderflow)` -> `exec.halt.stack_underflow`
- `EvmHalt(InvalidJump)` -> `exec.halt.invalid_jump`
- `EvmHalt(StateChangeDuringStaticCall)` -> `exec.halt.static_state_change`
- `EvmHalt(PrecompileError)` -> `exec.halt.precompile_error`
- `EvmHalt(Unknown)` -> `exec.halt.unknown`
- `ExecutionFailed` / `ExecFailed(None)` -> `exec.execution.failed`

境界仕様（固定）:
- user tx の `Revert/Halt` は `ExecResult(status=0)` で返す（Rejectedにしない）
- `Rejected("exec.*")` は `ChainError::ExecFailed` 経路に限定する
- Unknown halt 観測は `ExecFailed` だけでなく `Ok(ExecOutcome)` 境界でも実施する
- `produce_block` は prepare/commit分離を維持し、`system tx` は実行可能 user tx がある場合のみ実行する
- `system tx` 失敗時の非破壊対象は chain/mempool/index/receipt で、ops/health metrics 更新のみ例外として許容する
- `system tx` 連続失敗は `SystemTxHealthV1` で観測し、backoff中は prepare/system tx 実行を抑止する

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

## 運用ポリシー追記（2026-02-04）

- 公開APIから同期実行（`execute_ic_tx` / `execute_eth_raw_tx`）を削除し、書き込み導線は `submit_* + produce_block` に統一する。

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
