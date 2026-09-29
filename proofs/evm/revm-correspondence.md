# revm journal / CALL / REVERT の検証境界

## 固定対象

- 取得元: `humandebri/revm`、宣言 commit `50405a27f57f80a96ab9d91e93180bca22217a3a`。
- `revm` 34.0.0、interpreter 32.0.0。`revm-profile.json` は Rust/TOML/Cargo.lock
  295 ファイルの集約 SHA-256 と、gateway の Wasm 向け解決済み features を固定する。
- fork: `EVM_SPEC_ID = PRAGUE`。`execute_tx_on` で gas parameters と journal の spec も設定する。
- 本番 features: 各 revm crate は `std` のみ。`default-features = false`。
- 取得元 commit の Git blob とローカル Rust/TOML 294 ファイルを照合した。
  相違は下記 9 ファイル。journal/interpreter/handler の Rust ソースは取得元と一致した。

```text
Cargo.toml
crates/precompile/Cargo.toml
crates/precompile/src/bls12_381.rs
crates/precompile/src/id.rs
crates/precompile/src/interface.rs
crates/precompile/src/kzg_point_evaluation/blst.rs
crates/precompile/src/kzg_point_evaluation.rs
crates/precompile/src/lib.rs
crates/revm/Cargo.toml
```

主な相違は KZG/BLS12-381/P256 の依存・登録の feature 分離、関連テストと manifest の調整。
commit 名だけでは対象を特定できず、ローカル tree hash も必要。
現在 KZG (`0x0a`)、BLS (`0x0b`–`0x11`)、P256 (`0x100`) は登録されない。
**Prague の命令・gas 規則を使うが、Ethereum Prague の全 precompile 互換性は主張しない。**

追加 precompile（アドレス前方はゼロ、`kasane_precompiles.rs` も source hash 対象）:

| 末尾 | 機能 |
| --- | --- |
| `ffff0001` | ICRC wrapped token unwrap |
| `ffff0002` | native ICP withdrawal |
| `ffff0003` | ICP query |
| `ffff0004` | ICP update intent |

追加 gas の固定比率は 1/100。外部呼び出し、allowlist、intent dispatch、暗号処理は今回の証明対象外。

## ソースとの対応

Rust パスは `vendor/revm/crates/` が起点。

| 実装 | Lean | 範囲 |
| --- | --- | --- |
| `context/src/journal/inner.rs::checkpoint` | frame 前の履歴と後続 suffix の分離 | Rust の index 表現は未証明 |
| 同 `checkpoint_commit` | `commit_keeps_undo_entries` | depth のみ減らし、履歴を残す |
| 同 `checkpoint_revert` | `undo` / `undo_append` / `rollback_trace` | suffix の逆順再生 |
| 同 `checkpoint_revert` の `journal_i` / `log_i` | `revertAt` / `revert_at_checkpoint` | oldest-first suffix と log prefix の index 操作 |
| `context/interface/src/journaled_state/entry.rs::StorageChanged` | `save` / `undoEntry` / `restore_storage` | 旧 present value を同じ key に戻す |
| 同 `BalanceTransfer`、`inner.rs::transfer_loaded` | `step_inverse` / `valid` / `step` | 異なる account 間の正常移転、残高範囲の前提あり |
| `inner.rs` の `logs.truncate` | `rollback_logs` | checkpoint 前の logs のみ保存 |
| `handler/src/frame.rs` の CALL return | `parent_revert_after_child_commit` / `child_revert_preserves_parent` | 子 commit 後も親 REVERT で戻す |
| `handler/src/pre_execution.rs` の nonce/fee | `call_rollback_preserves_transaction_accounting` | CALL checkpoint 外の観測。順序はコード確認と実行テスト |

`rollback_trace` と `revert_at_checkpoint` は任意の有限な有効 action 列に対する証明。
各 action の逆操作も証明しており、「巻き戻しが正しい」という公理は置かない。
モデルの履歴は newest-first、Rust は oldest-first。`drain(...).rev()` と対応する。

`World` は既存・ロード済みの異なる 2 account の残高と、片方の storage 全体を観測する。
`Nat` を使い、移転は source 残高以下、target 加算と storage 書込みは U256 範囲内を前提とする。
同値を書き込む場合の履歴省略は観測上の恒等操作。touch/warm entry はこの projection では観測しない。

## 実装との比較

`bash scripts/verify-revm.sh` は以下を実行する。

1. vendored tree・本番 features・対応元ファイル・公式 fixtures の hash 確認。
2. Lean ビルドと全宣言の公理監査、既存の純粋関数 431 ケース比較。
3. 実際の `JournalInner` の `sstore` / `transfer_loaded` / checkpoint 操作と Lean を比較。
   初期 storage 3 通り × 移転額 3 通り、各 5 時点の storage 2 slot・残高・log 数/内容/順序・depth。
   合計 9 traces / 45 observations を完全一致で照合する。
   さらに storage 2 slot と transfer からなる長さ 5 の全 243 列について、
   子 checkpoint の commit/revert 両方を直接実装で検査する（486 traces）。
4. Ethereum Prague state fixture 38 ケースの state root と logs hash を照合。
5. Kasane の `execute_tx` と `RevmStableDb` でネスト CALL 12 ケースを実行。
   子 STOP/REVERT/INVALID × 親 STOP/REVERT × legacy/EIP-1559。
   storage・残高・logs・status・送信者 nonce・徴収額・受取残高を確認する。
6. CREATE と SELFDESTRUCT の親 commit/revert 4 ケースを stable DB まで検査。
   untouched account の commit skip と、REVERT 後の nonce・fee を別途検査。
   ICP update intent precompile の reverted subcall と再試行も実行する。

Kasane テストは毎回独立した thread-local stable memory を使う。
`init_stable_state` は領域を開き直す処理であり、再呼び出しは消去にならない。
公式テストには Kasane の base fee 加算や独自 precompile を入れず、独自仕様は別の実行テストで検査する。

## 検証で見つかった修正

従来の receipt は `gas_priority_fee.unwrap_or(0)` を使っていた。
再現: legacy の gasPrice=3、baseFee=1、priority 未指定の CALL。
実徴収は gasUsed×3、receipt は gasUsed×1 となった。
未指定時に gasPrice を渡すよう修正し、12 ケースで残高差と receipt の一致を確認する。
`legacy_price_exact` は入力変換後の価格が gasPrice と一致することを証明する。
同じ境界条件で `verified_core::fee::effective_gas_price` の Rust 実装にも
Verus の `--no-cheating` 契約を追加した。受理時の一般価格も Lean と Verus の
双方で `min(cap, base + priority)` と証明した。両証明は独立であり、
`execute_tx_on` の引数変換との結合はコードレビューと上記実行テストによる。
EIP-1559（cap=3、priority=1、base=1）は価格 2 のまま。

CREATE の親 REVERT テストでは、取り消された作成先が空 account として stable DB に
残る不具合を再現した。`RevmStableDb::commit` は untouched account も upsert していたが、
vendored revm の `CacheDB::commit` はこれを skip する。Kasane の判定関数に
`Skip` を追加し、Verus で untouched ⇒ Skip を証明した。Lean の状態モデルも
同じ分岐に合わせ、実際の CREATE と直接 commit テストで永続化されないことを確認する。
既存 canister の stable DB に過去の実行で残った空 account はこの変更だけで除去されない。
既存状態の棚卸しや修復は別の運用判断が必要。

## 残る証明義務と opcode 検証の評価

今回は **モデルの証明＋実ソースの対応レビュー＋実装との有限比較**。
Rust 全入力での journal/DB refinement、言語意味論、コンパイラ/Wasm の保存性は未証明。
hash 一致やテストの成功を refinement proof と呼ばない。

CREATE、SELFDESTRUCT、transient storage、warm/cold と refund、存在しない account、alias、
precompile、IC 非同期処理、任意の実装上のネスト深さは今後の対象。
Lean の任意長 trace 定理は、任意 EVM program の正しさを意味しない。
Rust テストは host 実行であり、Wasm 実行や mainnet deploy の証拠ではない。

- [EVMYulLean](https://github.com/NethermindEth/EVMYulLean) は Lean の EVM 意味論の候補。
  Prague と独自 precompile の差分を揃え、Rust からの接続を別途証明する必要がある。
- [Formal Land の v102 報告](https://formal.land/blog/2026/07/14/revm-v102-upgrade) は
  `rocq-of-rust` の抽出と simulation proof の先行例。今回の v34 と版が異なり、
  未証明補題とビルド対象外の links も報告されている。証明済みとして移植しない。
- 次の接続点は固定 v34 の `StorageChanged` / `BalanceTransfer` と checkpoint suffix の
  Rust 意味論からの refinement。Lean に統一する場合も抽出器の対応範囲と信頼境界が必要。
  今回、簡易 Rust 翻訳器や未検証の抽出結果は導入していない。

## 更新

ソース差分をレビューし定理とテストを更新した後だけ、profile を更新する。

```sh
python3 scripts/check_revm_verification_profile.py --print-current > proofs/evm/revm-profile.json
```

`model-sources.sha256` もレビュー後に更新する。fixtures は改変せず、差替え時に取得 commit・
SHA256SUMS・件数を更新する。Lean/profile ゲートは独立したローカルコマンド。
Rust integration tests は既存 CI の `cargo test ... --tests` にも含まれる。
