# EVM 接続部分の Lean 証明

Lean 4.30.0 / 標準ライブラリのみ。対象は Kasane の EVM 接続部分を写した数学モデル。
**Rust/Wasm 実装全体の正しさ、EVM 命令の意味論、モデルとの全入力での同値性は未証明。**
初期の接続モデルに加え、journal の限定した巻き戻しをモデル内で証明した。
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
既存の CI は Verus と Rust の実行テストを実行するが、Lean はまだ組み込んでいない。
スクリプトは対応元の SHA-256、`lake build`、公理監査、Rust/Lean の 431 ケース比較を実行する。
生成物は `.lake/` 内。Lean 側の評価は比較テスト用であり、証明には native evaluation を使わない。
`Audit.lean` は `KasaneEvm` 名前空間の全宣言の依存公理を走査し、Lean 標準の
`propext` / `Quot.sound` / `Classical.choice` 以外を拒否する。
`sorryAx`、独自公理、native evaluation 由来の公理も失敗になる。
監査の定理数には Lean が生成する補題を含む。

## 証明した性質と対応元

| モデル | 定理の内容 | Rust 対応元 |
| --- | --- | --- |
| `Fees.lean` | 有効価格の受理・拒否条件、受理時に `min(cap, base + priority)` と一致、base/cap/u64 上限。legacy の priority 未指定時は条件付きで cap と一致 | `verified-core/src/fee.rs::effective_gas_price` の Verus 契約と `revm_exec.rs` の入力変換 |
| `Fees.lean` | u64 同士の積は u128 に収まり、追加料金ゼロ時の総額は gas × price | 同 `l2_fee` / `total_fee` / `base_fee_reward` |
| `Fees.lean` | base fee 加算は nonce/code hash を保存。残高は U256 上限で飽和し、溢れなければ正確に加算 | `evm-core/src/revm_exec.rs::add_base_fee_portion_to_recipient` |
| `State.lean` | account 削除条件、code 未指定時の skip、storage のゼロ値削除は読み取り値を保存 | `verified-core/src/state_diff.rs`、`revm_db.rs` |
| `State.lean` | 書き込み一覧にないキーは不変。同一キーは最後の書き込みが有効 | `RevmStableDb::commit` の論理 map モデル |
| `Execution.lean` | Success/Revert/Halt から status/output/address/logs/gas/fee への射影 | `revm_exec.rs::execute_tx_on` の match |
| `Execution.lean` | commit 前の中断はモデルの DB を変えない。サイズエラーは commit 後に返る | 同関数の実行順序 |
| `Execution.lean` | commit がコントラクト観測を保存するという仮定を、REVERT の戻り値まで引き継ぐ | 条件付きの接続定理。巻き戻しそのものの証明ではない |

パスの `verified-core` / `evm-core` は `crates/` 配下。
明示的な定理は 32 個。`Nat` で整数値を表現し、Rust の型上限は定理の仮定と
`max64` / `max128` / `max256` で表す。任意の Nat を Rust の有効入力とは扱わない。

## Rust との対応と残る義務

対応を確認した出発点は Git `dce03b6ee64d3f075fcb84f9c9fe235c0ca73d49`。
`model-sources.sha256` が対応元 8 ファイル（manifest/lock を含む）を固定する。変更時はゲートが失敗する。
差分を読み、モデル・定理・比較テスト・この文書を更新してからハッシュを更新すること。
ハッシュ一致は同値性の証明ではなく、確認済みソースからの変化を検出するもの。

Verus は本番の `effective_gas_price` 実装について、受理時の価格が
`min(cap, base + priority)` と一致すること、および legacy の
`priority = cap`・`base ≤ cap ≤ u64::MAX` から `Some(cap)` を証明する。
Lean は `transactionPrice cap none base = some cap` を同じ境界条件で証明する。
`execute_tx_on` が未指定 priority を cap に写す箇所はコードレビューと実行テストで結ぶ。
二つの証明の間に機械的な言語間 refinement はない。

`rust_vectors.rs` は本番の `fee.rs` と `state_diff.rs` を直接コンパイルする。
価格 256 ケース、総額 144 ケース、reward 16 ケース、account 8 ケース、code 4 ケース、
storage 3 ケースを Lean と比較する。有限の比較なので、Rust と Lean の全入力での同値性は未証明。
storage の比較は `is_zero` の判定後を対象とし、U256 実装は検証しない。

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

特に `execute_tx_on` は `commit_state_diff` の**後**で `validate_execution_result_sizes` を呼ぶ。
従って返り値の `Err` だけから DB 不変を導くことはできない。
DB が cache か stable DB か、呼び出し元がどう扱うかは別途検証が必要。

外部の意味論を独自 `axiom` で既成事実にはしていない。必要な前提は定理引数またはこの一覧で明示する。
既存の `docs/verification/tcb.md` の `TCB-revm` 等はそのまま残る。
証明の監査方法は [Lean 公式資料](https://lean-lang.org/doc/reference/latest/ValidatingProofs/) に従う。
