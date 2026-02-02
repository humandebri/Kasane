修正計画 Spec（実装レベル）

目的
- ICのInstruction Limitで停止する要因を除去し、PoCでも「止まらない・復旧できる」状態にする。
- state_root / mempool / Storable復旧性 / export上限を、最小限の変更で安全に改善する。

非目標
- Ethereum互換のMPT/Verkleの実装。
- 完全に厳密なmempool ordering。
- 既存データの完全な意味復元（破損データは「読めるが不完全」を許容）。

対象範囲（現状の該当箇所）
- state_root: `crates/evm-core/src/state_root.rs`, `crates/evm-core/src/chain.rs`, `crates/evm-core/src/revm_exec.rs`
- ready_queue/rekey: `crates/evm-core/src/chain.rs`, `crates/evm-db/src/chain_data/ordering.rs`
- Storable::from_bytes: `crates/evm-db/src/**` の `from_bytes` 実装
- export上限: `crates/evm-core/src/export.rs`, `crates/ic-evm-wrapper/src/lib.rs`

---
## Task 1: state_root を O(changes) に置き換え

### 方針
- 全件走査をやめ、**「前回のstate_root + 今回の変更集合」**から決定的に更新する。
- Ethereum互換のMPTではなく、**Phase1用の決定的ハッシュ**として扱う。

### 変更仕様（具体）
1) `state_root` 計算式の定義
- 追加: `state_root = keccak256("ic-evm:state-root:v1" || prev_state_root || block_number || tx_list_hash || block_change_hash)`
- `prev_state_root` は親ブロックの `BlockData.state_root` を参照する。
- genesis（親ブロックが無い場合）は `[0u8; 32]` を使用する。

2) `block_change_hash` の定義
- 各Txの状態変更から `tx_change_hash` を作成し、**Tx順に連結**してハッシュ。
- `block_change_hash = keccak256("ic-evm:block-changes:v1" || tx_change_hash[0] || tx_change_hash[1] || ...)`

3) `tx_change_hash` の作成（実装レベル）
- `revm_exec::execute_tx` で `ExecutionResult` の `state` を取得済み。
- `state` は `HashMap<Address, Account>` で順序が不定なので、**ソートして決定順を確保**する。
  - `address_to_bytes` の `[u8; 20]` をキーに昇順ソート。
  - 各アカウントごとに、`changed_storage_slots()` を **スロットキー昇順**でソート。
- エンコード（バージョン付き、厳密固定順）:
  - 先頭: `b"ic-evm:tx-change:v1"`
  - 各アカウント:
    - `address(20)`
    - `flags(1)`: 0x01=selfdestructed, 0x02=empty+touched, 0x04=code_present
    - `nonce(8)`, `balance(32)`, `code_hash(32)`
    - `code_len(4)` + `code_bytes`（`info.code` がある場合のみ。無い場合は `code_len=0`）
    - `storage_count(4)` + `for each slot: slot_key(32) + present_value(32)`
- `tx_change_hash = keccak256(encoded)`

4) 実装変更点
- `crates/evm-core/src/revm_exec.rs`
  - `ExecOutcome` に `state_change_hash: [u8; 32]` を追加。
  - `execute_tx` 内で `tx_change_hash` を計算して返す。
- `crates/evm-core/src/chain.rs`
  - `produce_block` で `tx_change_hash` を `included` と同順で集計。
  - `execute_and_seal`（単発実行）でも同じ式で state_root を計算。
- `crates/evm-core/src/state_root.rs`
  - `compute_state_root_with` の全件走査は **debug専用**に移動。
  - `compute_state_root` は削除 or `#[cfg(feature = "full_state_root")]` で隔離。

### 互換性・影響
- `state_root` の意味が「全状態ハッシュ」ではなくなる。
- RPC/外部利用者向けに「Phase1の決定的root」として明記する。

### 追加テスト
- `crates/evm-core/tests/phase1_hash.rs`
  - `tx_change_hash` の決定性（順序が安定する）を検証。
  - `state_root` が `prev_state_root` と `tx_list_hash` と `tx_change_hash` で変化すること。
- `crates/evm-core/tests/phase1_produce_block_drop.rs`
  - 変更後も `produce_block` が落ちないことを確認。

---
## Task 2: ready_queue の全件 rekey を撤廃

### 方針
- ready_queue のキーを **base_fee 非依存**にする。
- `produce_block` 中で **先頭N件だけ評価**して選抜する（O(N)で上限あり）。

### 変更仕様（具体）
1) ReadyKey の新フォーマット
- `ReadyKey::new` を置き換え:
  - `(max_fee_per_gas, max_priority_fee_per_gas, seq, tx_hash)` を固定順にエンコード
  - 例: `fee_inv(max_fee)` -> `fee_inv(priority)` -> `seq` -> `tx_hash`
  - `is_dynamic_fee` は **priority=0** として扱い、EIP-1559でないTxは「max_fee=gas_price, priority=0」で統一

2) 取り出し戦略
- 新関数 `select_ready_candidates(state, base_fee, max_txs)` を追加。
  - `ready_queue.range(..).take(READY_CANDIDATE_LIMIT)` で候補を取得。
  - 候補の `StoredTx` から **effective_gas_price を計算**し、min_fee条件で落とす。
  - 有効候補を `effective_gas_price desc, seq asc` でソートし、上位 `max_txs` を採用。
  - 落としたTxは `TxLoc::dropped(DROP_CODE_INVALID_FEE)` で記録。
- `rekey_ready_queue_with_drop` は削除または無効化（呼び出しを削除）。

3) 追加定数
- `crates/evm-db/src/chain_data/constants.rs` に
  - `READY_CANDIDATE_LIMIT: usize = 256` を追加

4) 既存データとの整合
- `ReadyKey` のフォーマット変更により、既存 ready_queue の順序は無効。
- **アップグレード時に ready_queue/pending をクリア**（mempoolは正史ではない前提）:
  - upgrade hook で `ready_queue`, `ready_key_by_tx_id`, `pending_*` を全クリア。
  - クリアしたことをログに残す（例: "mempool cleared on upgrade"）。

### 追加テスト
- `crates/evm-core/tests/phase1_produce_block_drop.rs`
  - base_fee変動後に `rekey` を呼ばずとも処理が進むこと。
- 新規 `crates/evm-core/tests/phase1_ready_queue_selection.rs`
  - `READY_CANDIDATE_LIMIT` で上限が効くこと。
  - 有効候補の ordering が期待通りであること。

---
## Task 3: Storable::from_bytes の trap 根絶

### 方針
- `from_bytes` は **絶対に trap しない**。
- 壊れたデータは **「デフォルト値 + corrupted=true」** で返す。
- 書き込み (`to_bytes`) の検証は維持（不正な書き込みは防止）。

### 実装方針（具体）
1) 共通デコード補助
- `crates/evm-db/src/decode.rs` を追加:
  - `read_u32/read_u64/read_bytes` を **範囲チェック付き**で実装。
  - 失敗時は `None` を返す。
- すべての `from_bytes` で `decode` を使用し、失敗時は `Self::corrupt_default()` にフォールバック。

2) `corrupted` フラグ
- **永続フォーマットは変更しない**（末尾1byte追加はしない）。
- `corrupted` は **ランタイム判定のみ**で保持し、保存しない。
- 破損検知時は `metrics/errors` 増加や `meta.last_error` 更新で観測できるようにする。

3) キー型の扱い
- `AccountKey`, `StorageKey`, `CodeKey`, `ReadyKey`, `SenderKey`, `SenderNonceKey`, `TxId` などの**キー型**は
  - 不正長の場合、`keccak256("bad-key" || raw_bytes)` を切り詰めて固定長キーにする（決定的・衝突しにくい）
  - 併せて `ic_cdk::println!` で警告ログを出す

4) 既存テストの更新
- `crates/evm-db/tests/phase0_storable.rs` の `storable_rejects_wrong_length` は
  - `panic` ではなく「corrupted判定 or デフォルト値の返却」に変更。
- `crates/evm-db/tests/chain_data_storable.rs` も同様に調整。

---
## Task 4: export/query のハード上限制御

### 方針
- `max_bytes` に加えて **最大ブロック数の上限**をサーバ側で必ず守る。
- APIインタフェースは変更せず、内部固定値で制限する。

### 変更仕様（具体）
1) `MAX_EXPORT_BLOCKS` 追加
- `crates/evm-core/src/export.rs` に
  - `const MAX_EXPORT_BLOCKS: u32 = 64;`
- `export_blocks` 内に `blocks_emitted` を追加し、`>= MAX_EXPORT_BLOCKS` で停止。

2) Next cursor の扱い
- ブロック途中で停止した場合でも、`next_cursor` は **正しい seg/offset** を返す。
- 1ブロックも返せない場合は `Limit` を返す（max_bytes==0と同様）。

3) テスト
- `crates/evm-core/tests/export_api.rs`
  - `max_bytes` とは独立に `MAX_EXPORT_BLOCKS` で制限されること。
  - `next_cursor` の進み方が正しいこと。

---
## 追加で触るファイル一覧（想定）
- `crates/evm-core/src/state_root.rs`
- `crates/evm-core/src/revm_exec.rs`
- `crates/evm-core/src/chain.rs`
- `crates/evm-core/src/export.rs`
- `crates/evm-db/src/chain_data/ordering.rs`
- `crates/evm-db/src/chain_data/tx.rs`
- `crates/evm-db/src/**/from_bytes` 実装群
- `crates/evm-db/tests/phase0_storable.rs`
- `crates/evm-db/tests/chain_data_storable.rs`
- `crates/evm-core/tests/export_api.rs`

---
## リスクと対応
- state_root 非互換: 外部利用者に明記し、RPCレスポンスの文書に「Phase1の簡易root」であることを追記。
- ready_queue 再構築: upgrade時に ready_queue を再生成する処理が必要。
- corruptedフラグ追加: 既存バイト列は旧形式として解釈し、互換性を維持。

---
## 検証（必須）
- 新規/更新テストを全て実行。
- 既存 `export_api` / `phase0_storable` / `phase1_produce_block_drop` が通ること。
- `produce_block` が ready_queue の全件走査を行っていないことを確認。
