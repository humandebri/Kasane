# 実trait演算子の抽出・対応検証

## revmが使うtrait演算子の実抽出

固定revmのAND/OR/XORは`op1 & *op2`などのtrait演算子を使う。
前節で証明したinherent constメソッドとは実装経路が異なるため、別に抽出した。
`operator-lean/OperatorsGenerated.lean`は実trait演算子、owned/shared assignment、
標準u64 assignmentとRange iterator経由のループを含む。
`OperatorCorrespondence.lean`の15定理は全256ビット入力での可変配列アクセスの安全性、
4桁ループの終了、全4桁のAND/OR/XOR結果と全256個のbitのBool仕様を証明する。
途中状態は未処理の桁を保持し、残りの桁すべてに演算を適用する。
3個のRust入口はrevm命令と同じowned trait演算子を使い、
owned/shared assignmentと実標準u64 assignmentを通した結果に対応する。
命令のstack/gas/例外処理や通常Rust/Wasmの対応は、まだ証明していない。

固定RustのStep traitにあるforward_overflowing/backward_overflowingがAeneasの
traitモデルに未登録だった。既存のscalarモデル関数をそのまま使い、Stepの2フィールド、
unsigned/signed両instance、および抽出器のbuiltin field mappingを追加した。
`aeneas-step-overflowing-fields.patch`を第五の実験パッチとして保存する。
配布版バックエンドを変更せず、aeneas-source/backends/leanの実験用バックエンドを使う。
標準モデルと翻訳器の正しさは信頼境界に残る。このパッチのツール全テストは未実行。

`bash scripts/verify-operators-experimental.sh`で実ソース・backend/patchハッシュ、
再生成一致、警告検査、44宣言の公理監査と独立カーネル再検査に成功した。
実験バックエンドの1711 build jobs、生成パッケージの1714 jobsも成功。
同じ抽出器で既存のU256加減算とinherentビット演算ゲートを再実行し、
再生成一致・全入力証明・合同311宣言の監査とカーネル検査が成功した。
U256のAND/OR/XOR trait演算子は全入力の意味論対応が得られたが、
他の全U256演算子やEVM命令全体の証明済みとは扱わない。
AeneasのRange・array・scalar標準モデル、翻訳器と実験パッチは信頼境界に残る。

独立したOperatorCorrespondence名前空間でも、公理・sorry・native評価の負例3件を監査が拒否した。

全入力traitループ証明を含む新しい監査でも、公理・sorry・native評価の負例3件を拒否した。

## 固定revm実Stackの全入力読み取り対応

`operator-lean/StackGenerated.lean`は本番root Cargo.lockとevm-core依存profileから、
実`Stack::len/is_empty/peek`、Stack/U256/InstructionResultの型を抽出した定義。
未知定義になった標準`Vec::is_empty`も実体を追加includeし、公理を残していない。
固定Rustのstdlib Vecソースhash、root lock、Stack/InstructionResultソースと
実験バイナリhashを`verify-revm-stack-experimental.sh`で検査する。

`StackCorrespondence.lean`の6定理は全抽象Vec状態と全usize indexについて、
長さと空判定の正確な結果、peekの成功/StackUnderflow条件と正確な読み取り値を証明する。
`index < len`なら`data[len - index - 1]`を返し、それ以外ではStackUnderflowを返す。
有効なindexではusize減算と配列読み取りが安全で、抽出Resultが失敗しないことも含む。
len≤1024は仮定しない。Aeneas Vecのusize範囲内の長さ制約は表現型の条件。
Vec/array/scalarモデル、翻訳器・Rust compiler・実験パッチは信頼境界。

```sh
bash scripts/verify-revm-stack-experimental.sh
```

再生成一致、6定理の警告検査、実trait演算子も含む合同140宣言の公理監査、
独立カーネル再検査に成功した。新しいStackCorrespondence名前空間への
独自公理・sorry・native評価を混ぜた負例3件も拒否した。
この証明は実Stackの読み取り本体についての対応であり、raw pointerを使う
push/pop/dup/exchange、capacity・allocationの安全性、popn_top、命令のstack/gas/例外処理、
本番Rust/Wasmバイナリ全体の対応を含まない。

命令のpromoted MIRでもmutable borrowのCopyで停止した。LLBCでは
`context.interpreter`を一時変数へCopyしてからstackを再borrowしている。
単純なCopy→Move変換は行わず、診断を`diagnostics/revm-add-extraction.json`に保存した。

`RefCopyFragment.lean`は未採用の変換候補の局所断片モデルで、4個の明示定理を持つ。
`RefCopyAudit.lean`の18宣言監査と独立カーネル検査は
`bash scripts/verify-ref-copy-fragment.sh`で再現する。
実Rust/LLBC/OCamlパスの対応証明ではなく、命令証明数には含めない。

`TypeInfoGenerated.lean`/`TypeInfoCorrespondence.lean`は固定Charonの実Rust述語を
対象にした全入力対応8定理。`TypeInfoAudit.lean`は31宣言を監査する。
再現は`bash scripts/verify-typeinfo-experimental.sh`。
ASTのflag計算・native feature構成・GAT変換全体の正当性は未証明。

`GatBorrowFootprint.lean`はGAT借用比較に必要な位置集合の条件付きモデル。
実交差の過大近似、非交差/包含の十分条件、借用を無視した場合と未使用lifetimeを
借用と同一視した場合の反例など11定理を持つ。`GatBorrowAudit.lean`の11宣言監査と
カーネル再検査・負例3件は`verify-gat-borrow-obligations.sh`で実行する。
型から借用位置へのRust/LLBC対応、OCamlの比較処理、GATの意味論は未証明。
同ゲートは固定analyze_tyの具体的スライス/GAT対照検査も行い、未対応を再現する。

`LedgerDurationExtract/{Types,Funs}.lean`は固定ledger TimeStamp Add/Sub、u128→u64変換、
ナノ秒ラッパーの変更なしの分割抽出。`TypesExternal.lean`/`FunsExternal.lean`は
明示的な外部モデルであり、Rust stdの完全抽出ではない。DurationはU64秒・U32ナノ秒・
ナノ秒<10^9の証拠を保持する。TryFromIntErrorは生成されたIntErrorKindへの局所aliasで
単一フィールドのpayloadを保持し、既定のUnit消去を使わない。Debug/unwrapは
Unit formatterを使う局所観測モデルで、既定のFormatter公理に依存しない。
生成ファイルの名前解決がこの明示的モデルを参照することを意図している。

`LedgerDurationCorrespondence.lean`の明示14定理と外部モデルの範囲定理1個は、
このモデルの全入力について、整数変換・TimeStampの成功/失敗の必要十分条件、
飽和加減算、u64ナノ秒ラッパーの全域性を証明する。`LedgerDurationAudit.lean`は
補助宣言も含む59定理を監査し、通常の論理公理3個以外への依存を拒否する。
再現は`bash scripts/verify-ledger-duration-conditional.sh`。実Rustの境界テスト2件、
変更なしの再生成、警告検査、独立カーネル再検査、独自公理/sorry/native評価の
負例3件の拒否を含む。完全Duration抽出がTPatternで停止する負例も維持する。
Duration/単一フィールドalias/Debug/unwrapの実Rustとのrefinement、書式整形・panic text・
unwinding、Rustc/Charon/Aeneasの意味保存、IC/ledger/Wasm全体の対応は未証明。
詳細は`../ledger-duration-profile.json`。実命令・外部システム全体の証明数には加えない。

Duration全体を外部モデルにした経路に加え、`LedgerDurationSource/{Types,Funs}.lean`は
実std Duration型、from_nanos/as_nanos、実定数2個、TimeStamp Add/Sub等を変更なしで
抽出する。LLBCのStructured本体10個のうち関数8個・global initializer2個を毎回検査する。
外部型はU32とvalue<10^9の証拠を持つNanosecondsのみ。new_unchecked/as_innerは
明示的な境界モデルであり、実RustのTPattern/transmuteとのrefinementは未証明。
既存のDebug/unwrap観測モデルとTryFromIntErrorのpayload aliasも引き続き明示する。

`LedgerDurationSourceCorrespondence.lean`の19明示定理は、実Durationの除算・剰余・
キャスト・乗算・加算を使い、全入力での正確な各フィールド、往復、unchecked生成へ渡す値の
範囲とas_nanosの非overflowを証明する。同じ境界下でTimeStampの飽和加減算・
成功/panic必要十分条件・u64ナノ秒ラッパーも証明する。
`LedgerDurationRefinement.lean`の5定理は、旧モデルとの型対応の往復と2演算の一致を
示す。これは境界モデルと実Rustのtransmuteを結ぶ証明ではない。
旧モデルの数値定義を`LedgerDurationModel.lean`へそのまま分離した。生成enumの属性は
2組を同時importすると登録名が衝突するため、数値モデルだけを独立にimportする。

再現は`bash scripts/verify-ledger-duration-source.sh`。実Rust境界テスト3件、生成2ファイル
の再抽出一致、86宣言の公理監査、警告・独立カーネル検査、負例3件と完全Duration抽出の
TPattern未対応を検査する。既存の`verify-ledger-duration-conditional.sh`も再検証した。
固定ソース・モデル・未証明義務は`../ledger-duration-source-profile.json`。
Nanosecondsのunsafe transmute、実Rustのpanic/unwinding、Rustc/Charon/Aeneasの意味保存、
実Wasm/全EVM命令/IC・ledger外部挙動全体は未証明。

Nanosecondsのunsafe境界を調べるため、実new_unchecked/as_innerの両transmute本体も
選択抽出した。範囲付き完全型を消さずに借用/outlive分類だけを扱う未採用候補は
`../tool-patches/aeneas-u32-range-borrow-analysis.patch`。U32定数範囲のみに限定し、
実型と4境界範囲の20組の分類比較、不正/未対応12件の拒否、実unsafe本体の保持を検査した。
候補は完全型の型解析を通過するがPure型変換で停止する。両方向のtransmute自体も
未対応のまま厳格に拒否する。再現は`verify-u32-range-analysis-candidate.sh`。
この候補は未採用で、分類の意味保存や実Rust unsafe変換は証明していない。
既存Duration source/refinementゲートは再検証した。新しいLean定理は追加していない。
