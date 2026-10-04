# EVM 接続部分の Lean 証明

Lean 4.30.0 / 標準ライブラリのみ。対象は Kasane の EVM 接続部分を写した数学モデル。
**Rust/Wasm 実装全体の正しさ、EVM 命令の意味論、モデルとの全入力での同値性は未証明。**
初期の接続モデルに加え、journal の限定した巻き戻し、残高・範囲の保存、
論理 storage map への射影と巻き戻し、資産 precompile の呼び出し制限をモデル内で証明した。
実装は Prague の明示固定と legacy receipt 手数料の修正を含む。
詳細は [revm との対応・検証境界](revm-correspondence.md) を参照。

## 実行

リポジトリルートで実行する。

```sh
cargo check --workspace
bash scripts/verify-evm-proofs.sh
cargo test -p ic-evm-core --lib revm_exec::tests
```

`elan`、固定版の Lean、`rustc`、`shasum`、Verus が必要。統合ゲートは
`verify-verus.sh` の `--no-cheating` を通してから `verify-revm.sh` を実行する。
`verify-revm.sh` は独立した `target/evm-proof` を使用する。別 Cargo.lock の
PocketIC E2E と成果物を混ぜないためで、`CARGO_TARGET_DIR` で変更できる。
CI の `evm-proofs` job は `verify-revm.sh`、既存の `checks` job は Verus と Rust テストを実行する。
`verify-lean.sh` は対応元の SHA-256、`lake build`、公理監査、`leanchecker` による
保存済み証明の再検査、Rust/Lean の 439 ケース比較を実行する。
`verify-revm.sh` はサイズ検証 146 ケースと資産 precompile の入口 256 ケースも Lean と比較する。
生成物は `.lake/` 内。Lean 側の評価は比較テスト用であり、証明には native evaluation を使わない。
`Audit.lean` は `KasaneEvm` 名前空間の全宣言の依存公理を走査し、Lean 標準の
`propext` / `Quot.sound` / `Classical.choice` 以外を拒否する。
`sorryAx`、独自公理、native evaluation 由来の公理も失敗になる。
監査の定理数には Lean が生成する補題を含む。

## 証明した性質と対応元

| モデル | 定理の内容 | Rust 対応元 |
| --- | --- | --- |
| `Fees.lean` | 有効価格の受理・拒否条件、受理時に `min(cap, base + priority)` と一致、base/cap/u64 上限。legacy の priority 未指定時は条件付きで cap と一致 | `verified-core/src/fee.rs::effective_gas_price` の Verus 契約と `revm_exec.rs` の入力変換 |
| `Fees.lean` | 価格の受理・拒否の必要十分条件、飽和加算の結合則・上限・単調性。総額は一度の飽和と一致し、u64 範囲で実行料金以上。base fee reward は受理価格の実行料金以下 | 同 `effective_gas_price` / `total_fee` / `base_fee_reward` |
| `Fees.lean` | u64 同士の積は u128 に収まり、追加料金ゼロ時の総額は gas × price | 同 `l2_fee` / `total_fee` / `base_fee_reward` |
| `Fees.lean` | base fee 加算は nonce/code hash を保存。残高は U256 上限で飽和し、溢れなければ正確に加算 | `evm-core/src/revm_exec.rs::add_base_fee_portion_to_recipient` |
| `State.lean` | untouched account は書き込まず、touched の empty/destroyed account は削除。保存済みの空 account は読み取り時に不存在として扱う。code 未指定時の skip、storage のゼロ値削除は読み取り値を保存 | `verified-core/src/state_diff.rs`、`revm_db.rs` |
| `State.lean` | 書き込み一覧にないキーは不変。同一キーは最後の書き込みが有効 | `RevmStableDb::commit` の論理 map モデル |
| `State.lean` | 書き込みの合成・再適用の冪等性・初期値からの独立性・異なるキーの交換則・最後の削除。空 account の読み取りも冪等 | 論理 map と空 account 読み取りモデル |
| `Execution.lean` | Success/Revert/Halt から status/output/address/logs/gas/fee への射影 | `revm_exec.rs::execute_tx_on` の match |
| `Execution.lean` | 中断・サイズエラーはモデルの DB を変えない。モデル中の全エラーは commit 前であり、拒否後の再試行は同じ DB から開始する。成功の必要十分条件 | 同関数の実行順序。ホスト側の副作用は別の義務 |
| `Execution.lean` | output・log 数・各 log の topics/data サイズの必要十分条件と制限の単調性 | `validate_execution_result_sizes`。定理は任意の制限値、比較テストは本番定数 |
| `Execution.lean` | commit がコントラクト観測を保存するという仮定を、REVERT の戻り値まで引き継ぐ | 条件付きの接続定理。巻き戻しそのものの証明ではない |
| `Journal.lean` | 任意の有効 action 列で checkpoint index より後の oldest-first suffix を逆順 undo すると、観測状態と log prefix が復元される | vendored `JournalInner::checkpoint_revert` の index/drain/rev 形状。Rust との全入力 refinement は未証明 |
| `Journal.lean` | 有効な任意長 trace で残高合計・U256 範囲・未書き込み storage を保存。trace の合成・有限 frame 列の巻き戻し・再切り詰めの冪等性 | 既存の 2 account/storage/log projection。実際の depth・スタック・account lifecycle は範囲外。消費済み Rust checkpoint の再使用を許可しない |
| `Authorization.lean` | 許可されるのは非 static・外部許可済みの直接 CALL、target/bytecode 一致、Transfer value の場合に限る | 両資産 precompile の入口。ABI 検証・権限の由来・実送金は範囲外 |
| `Refinement.lean` | journal の forward/undo を論理 storage 書き込みに射影すると全キーの観測が一致。有効 trace の forward＋undo は元の map を復元 | **Lean のモデル間の対応証明**。Rust の diff 抽出との対応証明ではない |

パスの `verified-core` / `evm-core` は `crates/` 配下。
明示的な定理は 97 個（従来 36 個から 61 個増加。旧 `size_error_after_commit` は
現行仕様の `size_error_preserves_state` に置換）。`Nat` で整数値を表現し、Rust の型上限は定理の仮定と
`max64` / `max128` / `max256` で表す。任意の Nat を Rust の有効入力とは扱わない。

## Rust との対応と残る義務

対応を確認した出発点は Git `dce03b6ee64d3f075fcb84f9c9fe235c0ca73d49`。
2026-10-03 に `32160d7ec8c3e90f5730ef36ec4107b724229d2c` と検査時点の作業差分を確認した。
`model-sources.sha256` が対応元 9 ファイル（manifest/lock とサイズ定数を含む）を固定する。変更時はゲートが失敗する。
差分を読み、モデル・定理・比較テスト・この文書を更新してからハッシュを更新すること。
ハッシュ一致は同値性の証明ではなく、確認済みソースからの変化を検出するもの。

Verus は本番の `effective_gas_price` 実装について、受理時の価格が
`min(cap, base + priority)` と一致すること、および legacy の
`priority = cap`・`base ≤ cap ≤ u64::MAX` から `Some(cap)` を証明する。
Lean は `transactionPrice cap none base = some cap` を同じ境界条件で証明する。
`execute_tx_on` が未指定 priority を cap に写す箇所はコードレビューと実行テストで結ぶ。
二つの証明の間に機械的な言語間 refinement はない。

`rust_vectors.rs` は本番の `fee.rs` と `state_diff.rs` を直接コンパイルする。
価格 256 ケース、総額 144 ケース、reward 16 ケース、account 判定 8 ケース、
空 account 読み取り 8 ケース、code 4 ケース、
storage 3 ケースを Lean と比較する。有限の比較なので、Rust と Lean の全入力での同値性は未証明。
storage の比較は `is_zero` の判定後を対象とし、U256 実装は検証しない。

`RevmStableDb::commit` は以前、REVERT 後の untouched な作成先 account を
stable DB に書き込んでいた。vendored revm の commit と同じく untouched を skip し、
Verus 契約と Lean の `untouched_account_preserved`、CREATE の実行回帰テストで確認する。
この修正も DB 全体の refinement proof ではない。

過去に保存された空 account は CREATE の衝突判定を妨げることを実行テストで再現した。
`RevmStableDb::basic` と `basic_ref` は、nonce・残高がゼロで code hash が空の record を
revm に `None` として返す。判定の真理値表は Verus 契約と Lean の読み取り定理で扱う。
stable DB の byte decode、hash 判定からの接続は実装テストで確認し、形式証明の範囲外とする。

残る前提・未証明部分は次のとおり。

- `revm` が返す state diff、gas、nonce 更新、REVERT/HALT の巻き戻し、opcode、fork、precompile の意味論。
- Rust から Lean への翻訳の正しさ。実装の機械的抽出や refinement proof は未実施。
- `applyWrites` に渡す書き込み集合の完全性。stable map、byte codec、account/storage/code のキー対応、
  SELFDESTRUCT の削除展開、共有 code hash、イテレーション順序はモデル化していない。
- base fee 受取 account を DB または diff から正しく取得すること。加算定理は取得後の情報が対象。
  selfdestruct flag とその後の account 削除も範囲外なので、永続残高の保存則は主張しない。
- `finish` の `stopped` は commit 前の中断をまとめた入力。実際の停止条件や inspector/precompile の副作用は未証明。
  `sizesValid` も入力であり、各サイズ制限の実装を証明したものではない。
- receipt の hash、tx/block index、バイト列への encoding、永続化、trap 時の原子性、IC message rollback。
- 非同期 query の待機・再実行、cache から stable DB への flush、状態 epoch の整合性。

現行の `execute_tx_on` は `commit_state_diff` の**前**で `validate_execution_result_sizes` を呼ぶ。
`finish_error_preserves_state` は、このモデルが表す中断・サイズ拒否では supplied DB の commit が
実行されないことを証明する。Rust の RETURN/REVERT 出力超過・log data 超過・log 数超過を
共有 cache 上で再現し、拒否後の nonce・残高・storage と後続 transaction の永続化を検査する。
任意の `ExecError`、ホスト側の副作用、trap、IC message rollback 全体についての定理ではない。

検査中の query allowlist、update mode の選別、query transaction の active count snapshot と
最低 gas 検査の追加は source hash に含むが、Lean の資産 CALL 制限定理の対象ではない。
非同期処理とこれらの query/update 制御は引き続き上記の未証明境界に置く。

外部の意味論を独自 `axiom` で既成事実にはしていない。必要な前提は定理引数またはこの一覧で明示する。
既存の `docs/verification/tcb.md` の `TCB-revm` 等はそのまま残る。
証明の監査方法は [Lean 公式資料](https://lean-lang.org/doc/reference/latest/ValidatingProofs/) に従う。
