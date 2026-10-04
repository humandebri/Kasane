# Rustから抽出したLean定義の全入力対応

本番Rustをコピーせず、`rust/lib.rs` の `#[path]` で `verified-core` の実ファイルをコンパイルする。
CharonでMIRをLLBCへ、AeneasでLLBCをLeanへ変換する。
`verify-rust-lean.sh` は再生成した定義と保存済み定義の一致、ソースSHA-256、
全入力証明、公理監査、`leanchecker` による保存済み証明の再検査を要求する。

## 対応済み13関数

| 本番関数 | 全入力の対応 |
| --- | --- |
| `account_commit_decision` | 既存 `KasaneEvm.accountDecision` と一致 |
| `code_commit_decision` | 既存 `KasaneEvm.codeDecision` と一致 |
| `storage_commit_decision` | zeroならRemove、その他Insert |
| `account_is_empty` | 全u64 nonceと全Bool入力。balanceのゼロ/非ゼロ射影で既存モデルと一致 |
| `unwrap_dispatch_terminal_raw` | 全u64 statusで2または3と一致 |
| `unwrap_retry_transition_safe_raw` | 全u64の4入力で前3・後0・挿入1・error消去1と一致 |
| `unwrap_dispatch_transition_safe_raw` | 全u64の5入力でdispatchの4状態遷移に一致 |
| `unwrap_upgrade_recovery_safe_raw` | 全u64の5入力でqueue復旧・dispatch再queue・terminal保持の条件に一致 |
| `l2_fee` | 全u64×u64で成功し、返り値のNat射影が正確な積 |
| `base_fee_reward` | 全u64×u64で成功し、返り値のNat射影が正確な積 |
| `total_fee` | 全u64×u64×u128×u128で成功し、既存飽和加算モデルと一致 |
| `effective_gas_price` | 全u128×u128×u64で成功し、OptionのNat射影が既存モデルと一致 |
| `min_fee_satisfied` | dynamic/legacy両方の任意入力で比較式と一致 |

抽出対象13関数すべてに全入力対応定理を作成した。
`effective_gas_price` はdynamic feeの全u128×u128×u64入力について、
`Option`のNat射影が既存モデルと一致し、抽出定義が失敗しないことを証明する。
標準ライブラリの `TryFrom` / `Result::ok` の抽出はジェネリックな破棄処理で失敗したため、
本番Rustの最後の変換を「u64上限の検査後にキャストする」分岐へ明示した。
これは固定Rust標準ライブラリのchecked conversionと同じ条件・結果を持つ。
独自公理は追加しない。Verus契約では `None` の必要十分条件も追加し、
上限の境界値テストと既存の `TryFrom` を参照するプロパティテストが成功した。

## EVM整数演算の実装対応2関数

`word-rust` は本番root Cargo.lockと同じruint 1.20.0を呼び出す検証専用クレート。
本番依存に変更を加えない。`alloc,alloy-rlp,serde,std` のfeature構成、
同版の `src/algorithms/add.rs` のSHA-256、rootと検証用Cargo.lockをゲートで固定する。
依存グラフ全体の同値性を証明するものではない。

実際の `ruint::algorithms::carrying_add` と `borrowing_sub` の本体を抽出し、
全u64×u64×Bool入力で成功し、次の式を満たすことを証明した。Rは2^64。

- 加算: 結果値 + (桁上がりならR、その他0) = 左値 + 右値 + 入力carry。
- 減算: 左値 + (桁借りならR、その他0) = 結果値 + 右値 + 入力borrow。

結果値はU64なので常に0以上R未満。桁フラグを含む正確な整数演算の対応であり、
この配布版ゲートは4桁のU256処理を含まない。後述の実験版でU256加減算へ接続した。
EVMのADD/SUB命令、スタック、gas、例外処理全体への接続は未証明。
`WordCorrespondence.lean` に対応証明を保存する。独自公理を導入しない。

## 桁演算から256ビット演算への合成

`LimbComposition.lean` は抽出済みのruint桁演算を呼ぶLeanモデル。
同じ長さの任意の2つの桁列と任意の入力carry/borrowについて、処理が成功し、
結果の桁数が保たれ、桁フラグを含む正確な整数式を満たすことを証明した。
入力carry/borrowがfalseの場合、結果値は加算・減算のR^桁数による剰余に一致する。
結果値が常にR^桁数未満であることも証明した。R^4 = 2^256。

これはU256のループへ接続するための合成補題であり、手書きの桁列モデルと
この配布版パッケージには本番ruintのarray/update・range・mask処理への接続を含まない。
初期のU256本体の実抽出では、限定include後も標準のunchecked_addにある
ub_checksとunchecked演算がAeneas未対応。最適化MIRと単相化を組み合わせると、
今度は標準arrayのDefaultを処理するAeneas prepassで停止した。
その後、`tool-patches/` に記録した3つの実験パッチで実抽出が成功した。
`u256-lean/U256Loops.lean` は本番と同版・同featuresの実抽出について、
arrayアクセス・添字更新の安全性と、4桁ループ・wrapping加減算・検証用Rust入口の
全入力での成功と終了を証明する。`U256Arithmetic.lean` は同じ整数値定義を用い、
実配列更新とcarry/borrowを合成して、wrapping加減算と検証用Rust入口の
全入力での結果値が和・差の2^256剰余に一致することを証明する。
翻訳器・標準演算モデル・全MIR/Miri設定と通常ビルドの対応は信頼境界であり、
本番WasmやEVM命令全体の同値性は含まない。
実験パッチは配布版のCIゲートと分け、`verify-u256-experimental.sh` で
再生成一致・独自公理不使用・Leanカーネル再検証を検査する。

## 再現

前提は `curl`、Python 3、Rustup、Elan。ツールは本番の依存に加えない。

```sh
bash scripts/install-aeneas.sh
bash scripts/verify-rust-lean.sh
```

`toolchain.json` は公式配布の固定リリースとSHA-256、Rust nightly、Leanを指定する。
翻訳器・バックエンドは無視対象 `.local/proof-tools/aeneas`、
中間LLBCは一時領域、Lean定義と対応証明はこのディレクトリに保存する。
Lean/mathlib依存は `lean/lake-manifest.json` でコミットを固定する。

## 信頼する境界と未証明

Leanのカーネルが検査するのは抽出された定義についての定理。
Rust compiler、Charon/Aeneasの翻訳の正しさ、Aeneasの標準演算モデルとRustの対応は
このリポで証明していない。翻訳器を信頼する条件下で、上記13関数の
Rust意味論モデルと仕様の全入力対応が得られる。
本番Wasmとのバイナリ同値性、callerの値の算出、stable memoryのcodec/map、
呼び出し側のasync処理、revm全命令、IC replica、外部ledgerの実装対応を含まない。

2026-10-03: 本番13関数と依存2関数の再生成一致、Lean 4.31.0ビルド、90定理宣言の公理監査、
`Correspondence`・`WordCorrespondence`・`LimbComposition` の `leanchecker` が成功。独自公理・sorry・native評価公理への依存なし。

`.lake/`の一時的な負例でsorry・独自公理・native評価公理を監査が拒否することも確認した。

## 固定ledger実ソースの時刻構築・読み取り

`ledger-time-rust/timestamp.rs` は固定IC commitの公式ファイルを変更せず保存した。
SHA-256を`external/ledger-profile.json`の公式ソースと照合する。
検証用クレートの直接依存4個は公式Cargo.lockと同版だが、
推移的依存全体は検証用の別lockであり、IC workspaceや本番Wasmと同一とは主張しない。

`LedgerTimeCorrespondence.lean` の9定理は全u64入力の構築・読み取り・往復、
u64範囲の保持、および`TimeStamp::new`の成功値・入力条件・失敗条件を証明する。
後者の成功条件は`nanos < 10^9`かつ`secs * 10^9 + nanos ≤ u64::MAX`。
抽出は`-Coverflow-checks=yes`を明示するので、overflowによる失敗の証明は
このchecked算術profileに限る。公式release/canister-releaseやWasmのoverflow挙動との
同値性は未証明であり、この失敗条件を本番の仕様として転用しない。

`TimeStamp::add/sub`までの抽出も試したが、標準Duration内部のNanoseconds型が
`TPattern(U32, 0..999999999)`を含み、Aeneasが未対応で停止した。
範囲制約を削除した代替型や公理を使って証明済みにはしない。
時刻構築の証明から外部ledger全体の送金・重複判定・callbackの対応は導けない。

```sh
bash scripts/verify-ledger-time-experimental.sh
```

再抽出一致、9定理の警告検査、合同254宣言の公理監査を検査する。

`LedgerTimeWrappingCorrespondence.lean` の追加3定理は、同じ公式ソースを
`-Coverflow-checks=no`で抽出した生成について、全入力で成功条件が
`nanos < 10^9`であること、結果値が`(secs * 10^9 + nanos) % 2^64`になること、
範囲外nanosはassertionで失敗することを証明する。overflow設定による差を含め、
checkedとwrapping両設定の再生成一致・計12定理の警告検査・合同267宣言の公理監査・
両モジュールの独立カーネル再検査に成功した。
独自公理・sorry・native評価公理を混ぜた負例3件も監査が拒否した。
これはRust意味論の抽出profileの証明であり、実Wasmバイナリの対応は含まない。

## 固定U256実ソースのAND／OR／XOR

`bitwise-rust` は本番と同じruint 1.20.0・featuresを呼ぶ検証用クレート。
`BitwiseGenerated.lean` は実inherent const `Uint::bitand/bitor/bitxor` と
array/update・unchecked添字更新・4桁ループを抽出した定義。
`BitwiseCorrespondence.lean` の11定理は、全256ビット入力の配列アクセスの安全性、
終了、全4桁の正確な演算結果、および全256個のbit位置でのAND/OR/XORのBool仕様を証明する。
ループの途中状態も、未処理の桁を保持し、残りの各桁に演算を適用することを証明する。

`bash scripts/verify-bitwise-experimental.sh` はソース・本番features・固定バイナリ、
再抽出一致、警告検査、合同311宣言の公理監査とカーネル再検査に成功した。
これはinherent constメソッドの対応であり、別のtrait assignment実装やEVM命令の
stack/gas/例外処理、通常Rust/Wasmと抽出profileの対応は未証明。
翻訳器と4実験パッチは信頼境界。NOTの実抽出はraw pointerの参照未対応で停止した。
`diagnostics/bitwise-extraction.json`に停止位置と実ソースhashを保存し、公理で置き換えない。

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

## 具体Interpreter抽出の未使用traitメタデータ修正

以前の全単相化ADD/SUB LLBCは、array Default prepassで停止していた。
固定fixtureを調べると、除去済みtrait宣言を参照する空のpolymorphic Index実装が、
未使用メタデータとして残っていた。Default配列そのものの意味論エラーではない。
`aeneas-array-default-unreferenced-impl.patch`を第六の実験パッチとして保存した。
宣言グループ・type/function/global/trait・他のimplから参照されず、methodsも空の
メタデータに限り、Default-array判定の対象から外す。実装本体やメタデータは書き換えない。
参照された不正実装は引き続き拒否する。翻訳器パッチの正しさは信頼境界。

`bash scripts/verify-array-default-regression.sh`は固定された実LLBC fixtureに対し、
この前処理を通ること、および参照を追加した負例が拒否されることを検査し成功した。
これはEVM命令の証明ゲートではない。全単相化の次の停止位置はOptionに含まれる
可変参照のRErased型解析（TypesAnalysis.ml:229）。寿命を保持する部分単相化では
LLBCに5個のGAT警告が残り、Aeneasはmacros.rs:134の可変参照コピーで停止する。
CopyをMoveへ置き換えて変換成功とは扱わない。

同じ新バイナリで既存U256加減算とtrait AND/OR/XORの再抽出一致、警告検査、
公理監査（311宣言と44宣言）、独立カーネル再検査は成功した。
この第六パッチについてAeneas全テストは未実行。ADD/SUB命令本体の対応は未証明。

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

## 可変参照コピー融合の未採用候補

`tool-patches/aeneas-immediate-ref-copy-fusion.patch`は診断用の未適用候補。
同じpromoted MIRのADD/SUBで隣接する一時参照コピーと再借用だけを融合すると、
4関数のシンボリック変換まで進み、次にMemoryTrのGATで停止した。
Charonの5警告も残っており、命令の対応証明は成立していない。
独立した単純な参照プログラムの抽出成功は、この融合処理の検証にはならない。
意味保存検証が不足するため、
実験ツールを従来の6パッチ版に戻した。固定hashと既存ゲートはこの版を使う。
詳細は`diagnostics/revm-add-extraction.json`に記録した。

未採用候補の範囲検査は`bash scripts/verify-ref-copy-candidate.sh`で再現できる。
固定した実ADD/SUBのLLBCで6個の参照コピーを融合し、6種類×2関数の負例では
関数全体が構造的に不変であることを検査した。検査終了時には元のツールソースを
復元し、固定バイナリhashが変わっていないことも確認する。
これは意味保存の証明ではなく、命令対応の証明数は増えていない。

## 参照コピー融合の局所断片モデル

`operator-lean/RefCopyFragment.lean`の4定理は、NoRetagコピーと直後の再借用の
断片モデルを対象にする。任意のポインタ値・metadata・射影・再借用処理・world・
失敗について、死んだ一時変数を隠した観測が融合前後で一致する。
コピーはローカル環境のみを更新し、再借用の任意の状態変化はworldに残るという
定義の下での定理であり、RustやLLBCの意味論そのものの証明ではない。
実際の複合place、ポインタの有効性・provenance・生存期間、OCaml変換との対応は
未証明。候補の採用条件は満たしていない。

`bash scripts/verify-ref-copy-fragment.sh`は警告検査、18宣言の公理監査、
独立カーネル再検査、公理・sorry・native評価を入れた負例3件の拒否を確認する。

MemoryTrについてはLean変換だけでなく、固定CharonのLLBCに
`Error("Can't compute Self::Type0")`が既に含まれることも確認した。
Charon自身の診断が示すtrait除外を試しても、SharedMemory実装の
デフォルト関連型で抽出エラーになった。trait除外や型制約の消去は解決として
採用していない。まずフロントエンドのGAT/RPITITのbinderと等式制約を
正しく保存する必要がある。詳細は抽出診断JSONに記録した。

## GAT/RPITITフロントエンドの最小再現

`gat-diagnostic-rust`の3つのfeatureは固定Rustでいずれもコンパイルできる。
通常の参照を返す`plain`はCharon・Aeneas抽出まで成功する。
実MemoryTrと同じ`impl Deref<Target=[u8]> + '_`を返す`rpit`は
固定Charonで5個のGAT liftingエラー、明示関連型の`named`は2個の同エラーで停止する。
`bash scripts/diagnose-revm-gat.sh`は対照例の成功と既知の失敗を再現する。
診断スクリプトの成功はGAT対応や命令証明の成功を意味しない。
`plain`は戻り型が違うため、実MemoryTrの代替実装・対応証明には使わない。

固定Charonの`lookup_path_on_trait_ref`は`ItemClause`で必ず`None`を返す。
関連型の制約パスもGAT引数を表現しておらず、型族のbinderと等式制約を
正しく扱う処理が必要になる。revm・固定ツールの実装は変更していない。

失敗時もCharonは`has_errors=true`と宣言のエラー情報を持つLLBCを出力する。
診断はこれらを確認し、失敗した出力をAeneasへ渡さず削除する。
再現成功は既知の制約が確定したことを表し、命令の証明数は増えていない。

## 閉じたGAT等式の未採用Charon候補

`tool-patches/charon-gat-closed-item-equality.patch`は、GAT自身に宣言された
閉じた等式制約だけを参照する候補。trait・関連型・clauseのIDとSelf参照を照合し、
型引数への依存や親clauseの経路は処理しない。書き換え前の宣言を保持することで
取り出し中のtraitや書き換え後の制約消去に影響されず参照できる。

候補ドライバでは通常参照・RPITIT・明示GATがすべて無警告で抽出できた。
実ADD/SUBの別graphの抽出も`has_errors=false`・Error型0個となり、
MemoryTrのGATと生存期間binderが残っている。
2種類のGATを持つ例では`Bytes`のTargetが`[u8]`、`Words`が`[u16]`であることを
検査し、型引数に依存するTargetは依然として拒否される。

`bash scripts/verify-gat-closed-candidate.sh`は隔離された候補の5例と保存した
実ADD/SUB fixtureを検査する。固定ツールを変更しない。
候補の変換自体の正当性と一般のGAT置換は未証明・未完了。
固定Aeneasは正しく残ったGATを`SymbolicToPure.ml:224`で拒否する。
これはフロントエンドの進展であり、命令のLean対応証明にはまだ到達していない。
候補profileにhashと結果を保存し、通常ゲートのドライバとソースは固定版に戻した。

候補ソースのRust回帰検査は529件成功・既存の4件ignored。
単体20、Cargo11、crate data17、layout1、名前照合2、UI478を確認した。
UI生成データに対するOCamlの`dune runtest --force`もbyte/nativeの両modeで成功。
JSON/Postcard読取り、cross-formatエラー検査、名前照合4スイートを両modeで確認した。
全ターゲットClippyは`-D warnings`で成功し、自動修正は行っていない。
これらは変換の形式的な正当性証明には含めない。

初回は共有CARGO_TARGET_DIRを指定したため、fixtureの`cargo clean`が
共有キャッシュとツールを削除して失敗した。その結果は回帰判定に使わない。
固定ツールを保存コピーから復元してhash一致を確認し、共有指定を外して
既存のtool/fixtureごとのビルド先で全検査を再実行した。
通常ゲートのソースとバイナリは固定版で、候補は隔離したまま。

## 候補で使う実TypeInfo述語の全入力対応

`operator-lean/TypeInfoGenerated.lean`は固定Charonの実`TypeInfo::is_closed`と
その3個の述語、bitflags 2.9.1の実contains/from_bitsとflag定数を抽出した定義。
`TypeInfoCorrespondence.lean`の8定理は全flag・全mask・全Option/usize binder情報を
対象に、正確なBool結果と停止成功を証明する。free-var/self-clause/metadataの
maskは16/2/4で、`is_closed`はbinderがなく、これらを含まない場合にtrueを返す。

`bash scripts/verify-typeinfo-experimental.sh`でソースhashと固定ツールhashを検査し、
抽出を再生成して一致、8定理の警告検査、31宣言の公理監査、独立カーネル再検査、
独自公理・sorry・native評価を入れた負例3件の拒否を確認する。

抽出はCharonが提供する`--no-default-features`のライブラリ構成を使う。
rustc内部APIを含む構成では明示targetコンパイルでcompiler crateを参照できなかった。
対象述語自体にfeature分岐はないが、nativeツールの全feature/dependency構成との
同値性は未証明。ASTを巡回するTypeInfo::computeやキャッシュの整合性、
型の置換に対する意味論、GAT候補全体の正当性の証明でもない。
profileにこの範囲を記録した。候補は引き続き通常ゲートへ採用していない。

## 実ADD/SUBの候補による借用検査

閉じたGAT等式のCharon候補で得たエラー型のない固定fixtureと、参照コピー融合の
Aeneas候補を組み合わせると、`-borrow-check -abort-on-error -warnings-as-errors`が
成功した。実ADD/SUBとそのラッパーの計4個のStructuredBodyを記号実行する。
65個のOpaque関数宣言の内部は検査せず、呼び出しは型・シグネチャとbuiltinモデルで
扱う。unsafe stack実装の安全性やRustの参照意味論の証明にはならない。

`bash scripts/verify-revm-add-sub-borrow-candidate.sh`はソース・候補・固定ツールのhash、
エラー型不在、4本体と65個のOpaque宣言、MemoryTrのGAT/binder保存を確認する。
同じ入力を固定Aeneasがmutable borrowのCopyで拒否する対照検査も含む。
候補でのLean変換は4本体の記号変換を通過した後、保存されたMemoryTrの関連型を
`SymbolicToPure.ml:224`で拒否し、Lean出力を受理しないことを検査する。
fixtureを再抽出するゲートではなく、命令証明の定理数も増えていない。

候補Aeneasは既存6パッチ版に未採用の参照コピー融合パッチを追加してビルドし、
`.local/proof-tools/aeneas-ref-copy-candidate/aeneas`へ分離した。
profileは`tool-patches/revm-add-sub-borrow-profile.json`。通常ゲートのソース・バイナリは
元の固定hashへ復元済み。候補変換の意味保存、全本番依存graph、GATのLean変換、
全入力の命令結果・gas・stack対応、native/WasmおよびIC runtime対応は未証明。

## 全命令テーブルの実本体抽出診断

`revm-instruction-rust`は固定revmの`instruction_table_impl`から生成した診断用
ラッパー。実opcode定義とテーブルを照合し、150個の既知opcodeと106個の
未知opcode用処理を対象にする。PUSH1–32、DUP1–16、SWAP1–16、LOG0–4、
CREATE/CREATE2のconst引数もテーブルどおりに呼び出す。未知処理を含む151ラッパーは
EthInterpreter/DummyHostの別locked依存graphを使う。本番root graphとの同値性や
DummyHostと本番hostの対応は未証明。命令テーブル外で行うstatic gas計上も含まない。

`bash scripts/diagnose-revm-instructions.sh`は固定ソース・候補・通常ツールhashを検査し、
ラッパーを実テーブルの呼出先・const引数と字句単位で照合してRustでコンパイルする。
毎回、閉じたGAT等式のCharon候補で全85種類の実命令本体と151ラッパーを再抽出する。
全236個のStructuredBodyが名前単位で揃い、`has_errors=false`・Error型0個で、
MemoryTrのGAT/binderが保存されることを確認した。273個のOpaque関数宣言は残る。
全命令本体を完全にmonomorphizeした出力ではなく、実装のLean対応証明でもない。
10MBのLLBCと詳細トレースはゲートの一時ディレクトリに生成し、保存fixtureに依存しない。

この出力の候補Aeneas借用検査は、実`MemoryTr::slice_len`のdefaultメソッド呼出しで停止する。
戻り値は`core::cell::Ref<'_, [u8]>`だが、記号変数の戻り型は
`TraitClause0::ImpliedClause1::slice_len_ty<'_>`であり、`InterpPaths.ml:260`で不一致を拒否する。
診断ゲートはこの拒否と両方の型をトレースで確認する。型制約の消去や強制変換は
追加していない。ADD/SUBのみの診断成功を全命令の借用検査成功へ拡張できないことが
明確になった。profileは`tool-patches/revm-all-instruction-profile.json`。
候補変換の意味保存、GAT戻り型の等式、unsafe stackとhostの実体、gas/例外処理、
命令dispatch、native/WasmとIC外部runtimeの対応は未証明。

`slice_len`自身のLLBC宣言はOpaqueなTraitDefaultで、戻りシグネチャが具体的Ref。
GATのdefault型もRefだが、outlives・Deref・等式制約は残ることを診断ゲートで確認する。
抽出時のエラー型不在はLLBC全体の型整合性の証明ではない。
default型を全実装に対する無条件等式として使うとoverrideを失うため、
defaultメソッドの戻り型正規化とtrait呼出しの置換を文脈に応じて整合させる義務が残る。

さらにtrait宣言の`slice_len`メソッドシグネチャもdefault型と同じ具体的Refへ
正規化されていることを確認した。抽象的なtrait呼出しの受け取り先はGAT型のままで、
メソッド宣言・default関数・呼出先の間の置換整合性が未解決。

## 宣言RPITIT戻り型を保持する未採用候補

`gat-override-rust`は、既定の`slice_len`が借用スライスを返す一方、overrideは
所有Vecを返す有効な実Rust例。nativeテスト2件で、同じ1要素/空の入力に対して
既定実装が1/0、overrideが3/3を返すことを確認する。有限例の実行検査であり、
全入力証明ではない。古い候補のtraitメソッド宣言は既定の借用スライスへ展開され、
抽象GATの呼出先と不一致だった。実装ごとのGAT値はスライスとVecを区別していた。

`tool-patches/charon-rpitit-declared-return.patch`は宣言メソッドの直接の戻り型だけを
対象にする。固定Rust compilerの未正規化シグネチャで、RPITITのTrait射影が
そのメソッド自身に属する場合、既存のtrait proof・関連型ID・generic引数の
翻訳処理で射影を保存する。既定関数・実装関数の具体的戻り型やGAT値、
既存のDeref/等式/outlives制約は維持する。入れ子の戻り型や他のaliasは対象外。
全binder/override意味論の保存は未証明で、候補は通常ツールへ採用していない。

`bash scripts/verify-rpit-signature-candidate.sh`はhash、既存GAT例4個の抽出成功、
型引数依存GATの拒否、nativeテスト2件を検査し、旧/新候補の抽出を比較する。
新候補でtrait宣言が正しいSelf/関連型/lifetime射影を持ち、既定関数が借用スライス、
overrideのGATがVecのままであることを型単位で確認する。
最後に全命令を毎回再抽出する
`bash scripts/diagnose-revm-instructions.sh rpit-signature`も実行する。

新候補でも全85種類の実命令本体・151ラッパーの236本体がエラー型なしに抽出できる。
従来の`InterpPaths.ml:260`の戻り型不一致は越えるが、次にDeref呼出しで
`InterpBorrowsCore.ml:476`のGAT型比較が停止する。この処理はTTraitType全体の
等値を要求し、ADTのように生存期間引数を比較していない。GATの借用射影の
意味論・比較・Pure/Lean変換、unsafe stack/host/gas/例外/dispatch、
本番dependency graph、compiler/native/WasmとIC外部挙動は未証明。

候補ソースのRust回帰検査529件成功・4件既存ignored、
OCamlのbyte/native両modeでJSON924/Postcard924読取り、cross-format2件、
名前照合8実行が成功。全ターゲットClippyも`-D warnings`で成功した。
snapshotの更新や自動修正は行っていない。通常ソースと固定バイナリは復元済み。
profileは`tool-patches/rpit-signature-profile.json`、候補バイナリは
`.local/proof-tools/charon-rpit-signature-candidate`に分離した。

## GAT借用位置の義務と現行解析の反例

既存チェックを保持した診断runnerでは、Derefで比較される型は
`TraitClause0::slice_len_ty<'1>`と`<'3>`で、生存期間消去後は等しいことを確認した。
しかし現行の`TypesAnalysis`は両方のcontains_borrowをfalseと判定する。
`TTraitType`の分岐はGAT引数や隠れた借用を解析せず、初期の借用情報を返している。
等値チェックを消すだけでは、借用・返却やbackward関数の意味保存にならない。

`tool-patches/tests/GatBorrowAnalysisRegression.ml`は固定Aeneasの実analyze_tyを
呼び、同じ実抽出結果で既定関数の借用スライス戻り型はtrue、対応するGAT宣言は
falseとなることを直接検査する。どちらもADTの解析表を必要としない型なので、
空のADT表を使う。現在の未対応を再現する回帰検査であり、解析の正当性証明ではない。
診断パッチはログのみを追加し、元のsanity checkを維持する。専用のMainコピーを
別exeとしてビルドし、通常の固定main.exeやソースのhashは保持した。

`operator-lean/GatBorrowFootprint.lean`の11定理は、同一の選択済み型族/値について
実際に借用を持つ位置を任意の述語used、2つのregion viewをleft/rightとして扱う。
全候補位置での交差は実交差の過大近似であり、候補位置の非交差・包含は十分条件。
全位置が使われる場合の同値性、借用位置が空の場合の挙動も証明した。
生存期間をすべて無視したfalse交差/true包含の反例と、未使用の生存期間まで
実借用と同一視した場合のfalse positive/false negativeを含む。

これは条件付きモデルの義務であり、Rust/LLBC型をこの位置集合へ写す対応はない。
GAT引数だけでなくtrait/type引数やcapture、実装選択を含む完全な借用位置の列挙、
同じ型族のregion view間の対応、実Rustの生存期間・provenance・aliasing、
OCaml比較器の意味保存は未証明。GAT比較器の修正はまだ適用・採用していない。

`bash scripts/verify-gat-borrow-obligations.sh`は毎回最小例を新候補Charonで抽出し、
固定型解析の対照検査、診断runnerの厳格な失敗とregion view/借用flagを確認する。
さらに11モデル定理の警告検査・11宣言の公理監査・独立カーネル再検査と、
独自公理/sorry/native評価の負例3件の拒否を行う。監査した定理の公理依存は空だった。
scope/hashは`tool-patches/gat-borrow-obligations-profile.json`。
全EVM命令、native/Wasm、IC/ledger外部挙動の証明数を増やす結果ではない。

固定TimeStamp Add/Subは`Duration`をopaque型として**宣言を保持した分割抽出**が
可能になった。型を除外すると宣言参照が失われるため、除外経路は採用しない。
`operator-lean/LedgerDurationExtract`の生成2ファイルは実ソースから毎回再生成・一致検査
する。外部2ファイルは明示的なDuration/Debug/unwrapモデルと、実エラーの単一フィールド
payloadを保持する局所型aliasである。これらの実Rustとのrefinementは未証明。
この条件下で15個の明示的なモデル定理を追加し、全モデルDurationでのu64変換・
飽和加減算・panic条件、u64ナノ秒ラッパーの全入力を証明した。
`bash scripts/verify-ledger-duration-conditional.sh`は実Rust境界テスト2件、再生成一致、
59定理の厳格な公理監査、警告・カーネル検査、負例3件を実行する。
完全std Duration抽出の範囲型TPattern未対応も負例として再現し、制約の消去を認めない。
全Rust同値性・全EVM命令・IC/ledger外部挙動を証明済みにする結果ではない。
固定ソース・モデル・未証明義務は`ledger-duration-profile.json`。

`LedgerDurationSource`経路では外部モデルをDuration全体から範囲付きNanosecondsの
境界へ縮めた。実Duration型・from_nanos/as_nanos・定数2個とTimeStamp本体を
変更なしで抽出し、全モデル入力の除算/剰余・wide算術・各フィールド・往復・unsafe境界の
入力範囲を含む19条件付き定理を証明した。旧Durationモデルとの型の往復と2演算の
一致も`LedgerDurationRefinement.lean`の5定理で証明する。旧数値モデルは
`LedgerDurationModel.lean`へそのまま分離し、2組の生成enum属性の登録名衝突を避ける。

再現は`bash scripts/verify-ledger-duration-source.sh`。実Rust境界テスト3件、関数8個と
global initializer2個の抽出・生成一致、86宣言の公理監査・警告/カーネル検査、
負例3件に成功した。完全Nanoseconds抽出のTPattern失敗は負例として維持する。
Nanosecondsの実Rust transmute/validity、Debug/unwrap・エラーalias・panic、
コンパイル後のnative/Wasm、全EVM/IC・ledgerの外部挙動までの対応は未証明。
詳細は`ledger-duration-source-profile.json`。モデルの適合は実unsafe境界の証明を代替しない。

Nanosecondsの実unsafe関数new_unchecked/as_innerを追加選択すると、両方向の
型付きtransmute本体を変更なしで抽出できた。Nanosecondsがopaqueの経路では
固定AeneasのInterpExpressions.ml:983が両方向を拒否する。完全な範囲型を保持した
経路では、最初にTypesAnalysis.ml:486がTPatternを拒否する。

`tool-patches/aeneas-u32-range-borrow-analysis.patch`は未採用の分類候補。
U32 base・同じU32型の定数端点・0≤lower≤upper<2^32の範囲だけについて、
借用/outlive分類をbaseへ委譲する。元の型・範囲・値操作は書き換えない。
実Nanosecondsと4境界範囲の20組の分類比較、不正/未対応の範囲12件の厳格な拒否、
両unsafe本体のlocal・型・署名・cast方向の保持をnative checkerで確認した。
完全型は候補の型解析を通過するが、SymbolicToPureTypes.ml:195でTPatternを拒否する。

再現は`bash scripts/verify-u32-range-analysis-candidate.sh`。毎回両経路をCharonで
エラーなしに抽出し、固定版/隔離候補それぞれの失敗とLean出力がないことを検査する。
固定ツールのソース・バイナリは保持し、既存Durationの19条件付き定理と
5対応定理・86宣言監査も再検証した。詳細は
`tool-patches/u32-range-analysis-profile.json`。
候補の型解析全体の意味保存・Pure refinement型・実unsafe transmute・Rust/Wasm対応は
未証明。Aeneas全体のテストはこの候補では未実行。証明済み命令数は増やしていない。

未採用のPure範囲型候補では、U32定数範囲の両端をconst genericに保持し、Leanの
`{ rangeValue : Std.U32 // lower ≤ rangeValue.val ∧ rangeValue.val ≤ upper }`
へ出力する。実Nanosecondsの完全型（0..999999999）と実Duration.as_secsを
変更なしで再生成できた。既存の制約付きモデルとの往復・値保持・範囲保持と
実getterの全入力結果を5定理で証明し、11宣言の公理監査、独立カーネル再検証、
公理/sorry/native_decideの負例3件に成功した。
実unsafe new_unchecked/as_innerの両方向のtransmuteは引き続き厳格に拒否される。
候補翻訳器の意味保存、Rustのpattern型validity・レイアウト・unsafe変換、
native/Wasm・全EVM・IC/ledger外部挙動の対応は未証明。候補は通常ゲートへ未採用。
再現は`bash scripts/verify-u32-range-pure-candidate.sh`、固定ソース・候補・証明の
hashは`tool-patches/u32-range-pure-profile.json`。固定版ソースとバイナリは保持する。

実Nanosecondsの読取り方向のtransmuteを扱う、未採用の隔離候補を追加した。
実LLBCの正確な型名・非generic・U32範囲0..999999999と、native aarch64 Appleの
transparentレイアウト（サイズ/align4、単一fieldのoffset0、tagなし）を厳格に検査し、
一致した読取り方向だけをPure演算として抽出する。逆方向new_uncheckedは未対応。
実as_inner/as_secs/as_nanos本体3個と定数initializer1個を変更なしで選択し、
生成Leanコードに外部Nanoseconds型/読取りモデルやopaque公理を残さない。
その生成コードの全入力読取り・範囲・u128非overflowと結果を5定理で証明した。
既存モデルとのNano/Durationの型往復と2読取りの一致も6対応定理で証明した。
24宣言の公理監査・警告/カーネル検査・公理/sorry/native_decide負例3件に成功。
Nativeは実型の両cast本体、20分類比較、12範囲負例、3非Lean backend、24個の
メタデータ負例と逆方向/異なるscalarの拒否を検査する。実LLBCのtransparentを
壊した場合も厳格に失敗し、Lean出力を受け入れない。
実std Nanosecondsの妥当な5入力・安全な拒否2入力をRust/Miriで確認したが、
これはサンプルの実行証拠である。候補翻訳器とレイアウト解釈の意味保存、
unsafe生成の妥当性/UB、Rust/native/Wasmへの全入力対応・全EVM・IC/ledger外部挙動は
依然未証明。候補を固定版の通常ゲートへ採用していない。
再現は`bash scripts/verify-nanoseconds-read-candidate.sh`、固定範囲は
`tool-patches/nanoseconds-read-profile.json`。12個の固定ソースをビルド後に復元する。

unsafe生成方向を扱う別の未採用候補も追加した。実new_uncheckedの型付きtransmuteを
読み取り候補と同じ正確なnativeレイアウトで検査し、範囲内入力だけをSubtypeの値に
変換する部分的解釈として出力する。範囲外は`Result.fail Error.undef`で表現するが、
これはRustの実行結果/panicの主張ではない。実Rustは範囲外で即時言語UBとする。
RustのUBが取りうる全挙動や、候補の翻訳/レイアウト意味保存は依然未証明。
実Duration.from_nanos/as_nanos/as_secsとNanoseconds.new_unchecked/as_innerの5本体、
定数initializer2個を変更なしで選択でき、opaque Nano型/読取り/生成公理を残さない。
生成コードについて12定理を証明した。範囲内の生成/読取り往復、全u64入力での
from_nanosのunsafe前提成立とas_nanos往復、全Duration入力のu128非overflowを含む。
範囲外入力のundef定理は候補モデルだけの定理として区別する。旧条件付きNano/Duration
モデルへの型往復・範囲内生成・from_nanos/as_nanosの対応も9定理で証明した。
48宣言の厳格な公理監査、警告/カーネル検査、公理/sorry/native_decideの負例3件に成功。
Nativeは両方向のレイアウト条件、24メタデータ負例、逆方向/異なるscalar拒否、
20分類比較と12範囲負例、実両cast本体/署名の保持を検査する。不正レイアウト、
opaque型、固定版/読取りだけの旧候補の失敗も負例として再現する。
既存の実std境界テストはRust/Miriで再検証した。これはサンプル証拠で、
Rust/native/Wasm全入力対応・EVM全命令・IC/ledger外部挙動の証明を代替しない。
再現は`bash scripts/verify-nanoseconds-construct-candidate.sh`。固定範囲は
`tool-patches/nanoseconds-construct-profile.json`。固定12ソースを復元し、Mainもhash固定する。

実ledger TimeStamp加減算を、完全な範囲付きNano型と実new_unchecked/as_innerの抽出へ
接続した`LedgerDurationFull`経路を追加した。実本体10個と定数initializer2個を
Charonで毎回エラーなしに抽出し、隔離された部分的変換候補から分割Leanを再生成する。
生成ファイルは変更せずbyte一致を検査する。Nanoの手書き外部型/生成/読取り定義は
使わない。生成の範囲外は候補のundef表現であり、実RustのUBの全挙動を主張しない。
型の全入力に対する実Durationの算術・unsafe前提・フィールド・往復と、TimeStampの
成功/失敗iff・飽和add/sub・全u64ナノ秒wrapper結果を19条件付き定理で証明した。
既存のDuration全体モデルへの型往復と2演算対応も5定理で証明した。86宣言の公理監査、
警告/独立カーネル検査、公理/sorry/native_decideの負例3件、Rust/Miri検査が通った。
固定版のTPattern停止・読取りのみ候補のunsafe生成拒否・不正transparentレイアウトの
拒否を負例として維持する。実LLBCの両cast本体/署名とレイアウト/範囲ガードも検査する。
手書き外部定義として残るのはTryFromIntErrorのpayload保持aliasとDebug/unwrapの
観測モデルである。候補の型/レイアウト/部分的transmute翻訳の意味保存、これらの
エラー/formatting/unwindingの実Rust対応、backend primitive・native/Wasm対応と
全EVM・IC/ledger非同期外部挙動までの対応は依然未証明。通常ゲートへ未採用。
再現は`bash scripts/verify-ledger-duration-full.sh`、固定範囲とhashは
`ledger-duration-full-profile.json`。旧Nano外部モデル経路の19条件付き定理とは別の
実型・実cast抽出経路の証明で、全Rust/IC対応を代替しない。

TryFromIntErrorの手書きaliasを除いた`LedgerDurationTypedError`経路を追加した。
固定Rustでは実型がIntErrorKindを1つ持つnewtypeで、バックエンドの既定型対応は
Unitへ消去していた。未採用候補は実TryFromIntErrorとpayloadのclosedな定義を検査し、
6variantの名前・ID・fieldなし・Isize discriminant0..5が一致したときだけ通常の
実型抽出へ進める。型定義はAeneas自身のone-field newtype簡約で生成される。
自動生成されたExtractBuiltinLeanは変更せず、適用側で対象1型だけの登録を外した。
他のLean既定型対応は全て同一のままとNativeで確認した。変更/opaque型は厳格に拒否する。
実型/13不正定義の拒否と既存Nano範囲/レイアウト検査を隔離exeで確認した。
手書きNano型/生成/読取りとエラーaliasを使わず、実本体10個とinitializer2個を
毎回クリーン抽出し、生成ファイルを変更せずbyte一致させる。19条件付き定理と
5対応定理、86宣言の公理/警告/独立カーネル監査、負例3件が通った。
変更payloadとopaqueエラー型のLLBC負例も型ガードで拒否する。opaque化で削除される
共有payload型定義はSizeOf内に保持し、JSON読み込み失敗を型拒否の成功と扱わない。
実RustのU128→U64境界7入力について成功値と失敗payload PosOverflowをRust/Miriで
検証した（サンプル実行証拠）。残る手書き外部定義はDebug/unwrapの観測モデル。
候補の型簡約/翻訳/レイアウト/部分的transmute意味保存、formatting/panic/unwinding、
backend primitive・native/Wasm、全EVMとIC/ledger非同期外部挙動の対応は未証明。
候補は通常ゲートへ未採用で、Rustの全UB挙動も主張しない。
再現は`bash scripts/verify-ledger-duration-typed-error.sh`、固定範囲は
`ledger-duration-typed-error-profile.json`と`tool-patches/int-error-source-profile.json`。

`LedgerActualUnwrap`経路では、固定stdの実`Result::unwrap`も抽出した。
未採用Aeneas候補は`!`を返す呼び出しを残し、正常復帰側を空型消去する。
旧候補がErr分岐の後にOk payloadへ進んで失敗する再現を負例として保持する。
unwrapだけ既定モデルの登録を外し、他のLean既定関数が同一であることを検査した。
実本体11個とinitializer2個を毎回エラーなしで抽出し、生成Types/Funsはbyte一致する。
Err側の実呼び出しとon_unwind/drop/resumeはLLBCに保持されるが、既存Aeneasは
on_unwindを翻訳しないため、unwindingとdestructorの対応は証明していない。
19条件付きDuration定理・5対応定理は実unwrapの生成本体を使用する。さらに6定理で
任意のResult Neverの正常値が存在しないこと、継続の同値、失敗・発散の保持、
実unwrapの成功と条件付きpanic観測を検査した。公理監査は115宣言を列挙する。
panicメッセージは元のUTF-8バイト列として生成し、有限リストの長さをscalar_tacで
証明する。既定toStrのnative決定公理は使わない。16出力ケースでUnicode/NUL/引用符/
全256バイトも検査した。Rust/Miriでは実unwrapの成功4値・panic4例とメッセージを
観測したが、有限テストを全入力証明とは扱わない。
残る手書き外部観測はDebug formatterとunwrap_failed=panic（文字列整形を含まない）。
型/レイアウト/部分的transmute・Never翻訳の意味保存、実panic/formatting/unwinding、
backend primitive/native/Wasm、全EVMとIC/ledger非同期外部挙動は未証明。
再現は`bash scripts/verify-ledger-actual-unwrap.sh`。固定範囲は`ledger-actual-unwrap-profile.json`と
`tool-patches/never-call-profile.json`（extraction配下）。候補は通常ゲートへ未採用。

### Actual unwrap_failed and retained panic arguments (experimental)

固定Rustの`core::result::unwrap_failed`本体も抽出し、実unwrap・Duration・Nano・
TryFromIntErrorと合わせ14構造化本体（2 globalsを含む）を検証した。
`LedgerPanicPayload/{Types,Funs}.lean`は候補ツールの出力を無編集で保存する。
`Dyn`の隠れたSelf型・値・Debug実装と、共有参照経由の動的dispatchを保持する。
候補は固定Debug宣言の形だけを受け入れ、11不正predicate・22変更宣言・3非Lean backendを拒否する。
regionのoutlive情報は既存翻訳と同じく消去され、その意味保存は未証明。

Charonの通常Aeneas presetはpanic_fmt呼び出しを引数なしのPanicへ変換する。
限定候補ではpresetを偽装せず、残り14設定を明示し、panicの変換だけを外した
完全一致の入力設定を受け入れる。通常preset経路も再抽出し、設定差分が
preset・reconstruct_panic_calls・opaque指定・出力先だけであることを検査した。
新経路はソースの`panic_fmt(a2)`とNever継続を保持し、6 template bytes
`[192,2,58,32,192,0]`および2 formatting argumentsの受け渡しを残す。
52設定改変はLean出力前に拒否した。固定main/Charonを変更しない隔離ビルド。

19条件付きDuration定理・5精緻化定理・16 Never/動的dispatch/引数保持定理、
計40 named theoremsを検査した。`source_delegates_prepared_arguments`は外部panic
モデルを展開せずに準備した引数への委譲を示す。生成補助定理を含む153宣言の
公理監査はpropext/Quot.sound/Classical.choiceのみ許可し、独立kernel再検査、
axiom/sorry/native決定公理の拒否試験も通る。通常preset対照経路は144宣言。
Rust/Miriでは動的Debugのreceiver/実装/alternate flagと実unwrapを有限観測した。
これらの有限テストを全入力証明とは扱わない。

外部fmt constructorsはreceiverを閉じ込めたcallbackとtemplateを保持する明示モデル。
Str/エラーのfmt本体はno-op観測、panic_fmtはfail panic観測であり、実文字列整形、
Formatter flags/writer、unsafe pointer/ABI、unwinding/destructors、trapを証明していない。
Rust→LLBC→Lean意味保存、native/Wasm、EVM全命令およびIC/ledger非同期外部挙動も未証明。
候補を通常ゲートへ採用していない。
再現: `bash scripts/build-dyn-debug-candidate.sh`、
`bash scripts/verify-ledger-panic-payload.sh`。
固定範囲: `proofs/extraction/ledger-panic-payload-profile.json`、
`proofs/extraction/tool-patches/dyn-debug-profile.json`。

### Actual formatting constructors: next source obligation

`bash scripts/build-formatting-source-candidate.sh`と
`bash scripts/diagnose-ledger-formatting-source.sh`は、前節の外部コンストラクタを
実ソースへ進める診断。new_debug/new_display/Arguments.newを含む17構造化本体を
固定Rustからエラーなしで抽出した。Argument/ArgumentType/Argumentsの実フィールドを保持し、
前節からさらに2 type/3 function builtinを外す（error/unwrapを含め計3/4）。他の登録は不変。
両argument constructorのNonNull::from_ref/cast、trait method→FnPtr reificationと
FnPtr transmute、Arguments.newの2 pointer transmuteを元のLLBCで検査した。

29個の解決済みFnPtr型をNative側で確認した。隔離候補は実ArgumentTypeのfield翻訳で
`Arrow types are not supported yet`となり、Leanファイルを生成する前に停止する。
借用/型検査を無効にする等の5設定変更も入力profileの段階で拒否した。
これは障害の再現成功であり、コンストラクタの対応証明の成功ではない。
次の義務はABI/unsafe/region binderを保持する関数ポインタの翻訳、trait methodの
関数ポインタ化、型消去されたreceiverとformatterの対応、共有・可変参照を含む
indirect call/戻り状態の意味保存、NonNull/各transmuteのvalidityとlifetimeである。
非CFIの固定targetの本体を検査したので、CFI/KCFIの別経路の証明にもならない。
固定範囲は`tool-patches/formatting-source-profile.json`。前節の14本体・40条件付き
定理の候補と固定ツールを保存し、この診断を証明済みの範囲に加えない。

### Closed function-pointer source signatures in Pure (experimental)

`aeneas-closed-fnptr-signature.patch`は、型変数/const引数/trait参照等を含まない
Rust ABIのFnPtrを、元のsignature全体を持つPure builtin markerへ運ぶ。
unsafe/variadic/ABI、各ref kind、bound regionのbinder/番号/属性、input/outputを保持し、
通常のPure arrowやUnitに変換しない。markerは閉じた型なので型代入が署名内の
自由変数を隠さない。型変数を含むsignatureには代入を扱える表現が必要なため拒否する。
一般のFnPtr支持・ABI解釈・callable loweringの完了ではない。markerの値/callの
意味保存とLean抽出は未実装で、extract_tyは明示的に拒否する。

Nativeでは実17本体から29 FnPtr出現を検査し、13閉signatureを全metadata一致で
保持した。16 open出現、12不正/open signature、3非Lean backendを拒否し、
safe/unsafe markerの区別と閉markerへの型代入不変も確認した。
これらは有限Nativeテストであり、任意ASTに対する証明ではない。

実17本体の候補はArgumentTypeの型宣言翻訳を通過した。次はfn item作成時の
`ty_is_rty`がlate-bound regionを拒否する（InterpUtils.ml:167）。さらに
Arguments.newの配列参照→NonNull transmuteがInterpExpressions.ml:987で拒否される。
元の型宣言障害も前候補で再現した。翻訳進捗ログは14/17になるが、これを対応証明
の完了数/割合として扱わない。5不正設定は入力profileで拒否し、Lean出力は0ファイル。
次の義務はfn itemの正しいbound-region scope、trait methodのreification、
receiver/function pointerの関係、NonNullのvalidityとtransmute/indirect callである。

再現: `bash scripts/build-closed-fnptr-candidate.sh`と
`bash scripts/diagnose-ledger-closed-fnptr.sh`。
固定範囲: `tool-patches/closed-fnptr-profile.json`。
既存の40条件付き定理へ新しい証明を加えていない。固定ツールと前候補は維持した。

### Function binder scope and region erasure (experimental)

fn-region候補では関数型内のregion binderを数え、Bound(depth,id)を範囲付きで
検査する。通常型のrty検査、自由regionの消去、範囲外/erased regionの拒否を維持した。
実4 FnDef/29 FnPtrの署名は変更しない。36 depth/id組とempty nested binderをNative検査した。
最初にrty predicateだけを直しても停止し、追加診断で実行時のregion消去が
FnDefの3 late-bound argumentsまでRErasedに変えることを確認した。
body/local消去にbinder保持を適用し、Charon MLのerase callbackもshift_substが
一時的な負のdepthで渡す局所変数を残すようにした。自由/外側変数の消去は維持する。

この候補は実コンストラクタのrty停止を越える。次はtrait method→FnPtr reification
castがInterpExpressions.ml:987で厳格停止する。Arguments.newのref→NonNull
transmuteも同箇所で停止する。元のrty失敗は前候補で再現した。17本体を再抽出し、
5不正設定を拒否した。Leanファイル出力は0。翻訳ログ14/17を対応証明の完了数としない。
Charon MLを自身のDune projectで明示ビルド/強制runtestし、JSON/Postcard読込、
cross-formatエラー検査、name matcher検査が通った。vendored aliasでのno-opは結果に数えない。

`operator-lean/FnRegionErasure.lean`はshift_subst/erase callbackの整数深さモデル。
free消去、local bound保持、outer bound消去、erased保持、shift合成、erasure冪等性の
6補題を全入力で示す。公理監査/独立kernel/warning-as-errorとaxiom/sorry/native負例が通る。
モデルのinteger variableに関する証明で、ML visitor/binder traversal、型/borrowの
健全性、Rust→LLBC→Lean、reification/unsafe pointer ABI、IC/ledger外部挙動の証明ではない。
以前の40対応定理へRust本体の新しい定理を追加したとは数えない。

隔離候補の`charon-trait-fndef-signature.patch`は`lookup_fndef_sig`のTraitMethodを
既存のitem binder→method binder代入で解決する。外側のfunction region binderは残す。
高階trait region binderは未対応としてNone、trait/method欠落もNoneを返す。
実new_debug/new_displayの2 reificationでは、解決署名と変換先FnPtrが構造的に完全一致した。
入力/出力型、全region binder、Rust ABI、safe/non-variadicを比較し、安全性/ABI/variadicの
変異は異なる署名として区別した。これは実ソース2箇所の有限Native検証で、
署名代入器の全入力正当性や関数項目→ポインタの値/呼出しの意味保存の証明ではない。
Charonの実MIR変換ではReifyFnPointer/UnsafeFnPointerがCastKind::FnPtrへ変換される。
Aeneasのsymbolic cast評価、TraitMethodのVaFnDef翻訳、open FnPtrのcallable表現は
引き続き未対応で、この追加だけでreification停止を越えたとはしない。

再現: `bash scripts/build-fn-region-candidate.sh`、
`bash scripts/diagnose-ledger-fn-region.sh`。
固定範囲: `tool-patches/fn-region-profile.json`。
固定binary/源コード/前候補は維持し、これらの候補は通常ゲートへ未採用。

### Exact function-item cast in the symbolic AST (experimental)

`aeneas-fn-item-reification.patch`は既存のsymbolic UnaryOp経路でCastFnPtrを保持する。
source operand型がcast元と一致し、function itemの署名解決結果がcast先FnPtrと完全一致し、
Rust ABI/non-variadicで、型引数の個数と局所region scopeが有効な場合だけ受け付ける。
関数identityを表すVaFnDefとcast演算は消さず、Pureのarrowや実行可能なpointerへ置換しない。
一次仕様: [Rust Reference: function item coercion](https://doc.rust-lang.org/reference/types/function-item.html#coercion)。
固定CharonではReifyFnPointer等がCastKind::FnPtrへ変換される。unsafe pointer同士や
ABIを変えるcastなど、この条件以外のケースはこの候補で対応したとはしない。

実17本体でnew_debug/new_displayの2 reificationはこの検査を通った。
14 target変異のNative負例と、実LLBCのcast先ABI/safety/variadicを変えた3負例は拒否した。
追加で発見した停止は、関数型内の局所Boundをty_regionsがfree regionとして扱うこと、
local_erase_regionsが関数binderを消すこと、mk_bottomのETY検査が局所Boundを拒否することだった。
`aeneas-fn-region-inventory.patch`は関数型のbinder内だけscopeを数え、局所Boundを
自由region集合に入れない。Freeと終了済みregionの追跡は維持し、ordinary/out-of-scope/
erased/body regionの既存の拒否とempty nested binderをNative検査した。
`aeneas-fn-value-erasure.patch`は値型の消去を既存のbinder保持処理と揃える。
`aeneas-fn-erased-type-scope.patch`は関数型内の有効な局所BoundをETYで保持し、
関数内のFreeと通常型のFree/Bound/staticの拒否を維持する。実33 function型でこれらを確認した。
これらはMLの有限回帰検証で、visitorやRustのborrow/lifetime規則の全入力証明ではない。

次の厳格停止はInterpExpansion.ml:731のFnPtr greedy borrow expansion。
TypesAnalysis.mlのTFnPtr分岐がsignatureの入力/出力へ再帰し、引数の参照を値のborrowとして
報告する一方、greedy expansionにはFnPtrの場合がない。ここを無効化したとはせず、
`Unreachable`を残る実ソースの障害として再現する。Arguments.newのref→NonNull transmuteも未対応。
以前のrty/region collection/代入型不一致/ETY停止はこの候補で越えた。
17本体を新規再抽出した完全診断ゲート、前fn-region候補のcast停止、5設定負例が通った。
Charon standalone ML testsも候補適用中に明示ビルド/強制実行して通った。
Lean出力は0、Rust本体の新しい対応定理も0。既存6 integer-depthモデル補題の監査/
独立kernel/負例は維持されるが、reificationやpointer runtimeの証明へ数えない。

再現: `bash scripts/build-fn-reify-candidate.sh`、`bash scripts/diagnose-ledger-fn-reify.sh`。
固定範囲: `tool-patches/fn-reify-profile.json`。前候補と固定Mainは維持し、通常ゲートには未採用。

### Function-pointer value borrows (experimental)

`aeneas-fnptr-stored-borrows.patch`はTypesAnalysis.analyze_full_tyのTFnPtr分岐だけを
変更する。呼出しの入力/出力型への再帰でborrow flagsを増やす処理を外し、既に得た
`ty_info`をそのまま返す。空のinfoにリセットしないため、外側の参照や先行するtuple fieldの
borrow flagsを維持する。後続fieldの解析も継続する。function signature/ABI/safety/binderの
metadataには触れず、free/ended regionの集合検査も前候補のまま維持する。
一次仕様ではfunction pointerはcodeへのpointerで、環境を捕捉するclosureとは別の値である。
[Rust fn documentation](https://doc.rust-lang.org/std/primitive.fn.html)、
[Rust Reference](https://doc.rust-lang.org/reference/types/function-pointer.html)を確認した。
これは値が保持する借用の分類であり、関数呼出しの引数/戻り値の寿命制約を外す変更ではない。

実29 FnPtrでstored borrow flagsが全てfalseであることと、shared/mutable外側・隣接field
58組およびnested contextのflagsが対応するscalar baselineと一致することをNative検査した。
署名は検査前後で同一。既存33 function region/ETY/消去と17 cast負例も維持した。
固定nightly-2026-09-17で`ledger-time-rust/tests/fn_value_source.rs`の3観測をRust/Miriで確認した。
引数が消えた後のpointer再利用、戻り参照のidentity、外側の可変参照によるpointer更新を検査する。
捕捉closure→FnPtr(E0308)、寿命切れの戻り参照(E0597)、使用中の可変外側参照の破壊(E0506)は
3つの独立したcompile-fail fixtureで拒否した。これらも有限検証であり、ML分類器やborrow checkerの
全入力健全性、Rust→LLBC→Leanのrefinementを証明したとは扱わない。

新規17本体抽出の完全診断ゲートが通り、前fn-reify候補のInterpExpansion731停止を再現した。
新候補はそのgreedy expansion停止を越え、new_debug/new_displayの実safe FnPtr→
unsafe FnPtr(NonNull<()>,&mut Formatter)のtransmuteで厳格停止する。
Arguments.newの参照→NonNull transmuteも未対応。実castの3 ABI/safety/variadic負例と
5設定負例は拒否する。CFI/KCFI用の別経路やFFI/ABI全般をこの結果へ含めない。
固定core/src/fmt/rt.rsではreceiverの型に関するSAFETY理由と、CFI時にはtrampolineを使う
分岐が明記されている。型metadataの保持だけではそのpointer/呼出し義務を完了できない。
Lean出力は0で、実Rustの新しい対応定理も0。既存6 integer-depth MODEL補題は独立した範囲で維持する。

再現: `bash scripts/build-fn-value-candidate.sh`、`bash scripts/diagnose-ledger-fn-value.sh`。
固定範囲: `tool-patches/fn-value-profile.json`。固定Mainと前候補を維持し、通常ゲートへ未採用。

### Function-pointer representation transmute (experimental)

固定nightlyのcore/src/primitive_docs.rsのABI節は、同じABIのfunction pointer型間の
transmute自体がwell-definedであることと、その後のcallがUBになり得ることを区別する。
同じファイルの& T/NonNull<T>のABI条件とcore/src/fmt/rt.rsのArgumentType invariantも読んだ。
fmtの実コンストラクタは同じTのreceiverとfn(&T,&mut Formatter)を組にし、非CFI経路では
fn pointerをunsafe fn(NonNull<()>,&mut Formatter)へ変換する。call時の正しいT/存続期間/
reference validity/argument ABIは、pointerの表現変換だけからは従わない。

`aeneas-fnptr-transmute-representation.patch`はFnPtr→FnPtrのCastTransmuteを既存の
symbolic UnaryOpとして保持する。cast元型とoperand型の一致、両側のrty scope、Rust ABI、
non-variadic、unsafe target、同じarity/result型を要求する。この条件はcall ABI互換性の
判定ではなく、限定されたpointer表現の記録条件。callable Pure関数へ置換しない。
実2 transmuteを受け付け、22 Native source/target対照と、実cast先だけを変えた3 ABI/safety/
variadic対照を拒否した。通常の参照→pointer transmuteは受け付けない。

次に局所Boundをregion_in_setへ渡していたty_has_regions_in_predの停止を確認した。
`aeneas-fn-region-predicate.patch`はfunction binder内だけscopeを数え、有効な局所Boundを
自由region predicateへ渡さない。33実function型、Free集合の一致、不一致、erased predicate、
範囲外localの対照をNative検査した。前候補のfree/ended region inventory、ETY、outer borrow
保持、reification/cast否定例も維持する。ML visitor/分類器の全入力健全性はまだ証明していない。

`ledger-time-rust/tests/fn_transmute_source.rs`はprivate receiver/callback pairを用いて
固定coreの非CFI方式を再現するprobe。scalar/copy、内部にslice metadataを持つreceiver、
formatter flagsとreceiverへの副作用、fmt::Error、ZSTの5観測が固定Rust/Miriで通った。
これは元のcore/src/fmt/rt.rsそのものの抽出証明ではない。
`tool-patches/tests/fn_transmute_receiver_fail.rs`はu8 receiverとu64 callbackを誤って組にする
Miri専用対照で、&u64のallocation bounds違反として拒否された。nativelyは実行しない。
一時manifestのMiri runnerが古いcwdを再利用しないよう、負例のtargetを一時directory内へ分離した。
finite observationsであり、正しいreceiver pairについての全入力call/refinement定理ではない。
CFI/KCFI、他ABI、FFI/linking一般、IC/Wasmの実行をここで検証したとはしない。

完全ゲートは実17本体を再抽出し、前fn-value候補のFnPtr transmute停止、5設定負例も再現した。
新候補では両コンストラクタのsymbolic段階が進み、次はSymbolicToPureTypes.ml:487の
higher-ranked function-item型で停止する。元コードのTFnDef分岐はbinder_regions=[]を要求し、
TraitMethodも未対応。open FnPtrのcallable表現、trait method値の抽出、transmuteのPure意味論、
Args.newの参照→NonNullも引き続き未対応。Lean出力/新しいRust対応定理は0。
既存6 integer-depth MODEL補題の監査/kernel/負例は維持し、これをpointer証明へ加算しない。

再現: `bash scripts/build-fn-transmute-candidate.sh`、`bash scripts/diagnose-ledger-fn-transmute.sh`。
固定範囲: `tool-patches/fn-transmute-profile.json`。ABI一次資料もhash固定し、前候補/固定Mainを維持。
通常ゲートには未採用で、unsafe callや外部formattingを証明済みとは扱わない。

### Trait function-item type templates (experimental)

固定coreのdebug/display constructorに現れる4 TFnDefを、Pureのarrowへ潰さず保持した。
trait/method ID、局所region binderとregion引数を閉じたmetadataに残し、自由Self型と
trait clauseの証拠は通常のPure generic argumentsへ置く。これにより、代入やvisitorが
自由変数を見落とすopaque metadataにはならない。対象は単一の自由Self型・自由clause、
空のtrait region binder、型/const/trait method引数なし、有効な局所region引数に限る。
通常のFunに対する既存forward変換は維持する。callable抽出は明示的に拒否する。

Native検証では4実型のsource再構成一致、PureでのSelf/trait証拠の代入、
20自由変数の付替えroundtripを確認した。60範囲外の型変形と3他backendを拒否した。
これは有限の回帰検証であり、全Rust型についての変換定理ではない。
実17本体の抽出は以前のSymbolicToPureTypes:487停止を越え、
SymbolicToPureExpressions:609の「TODO: function casts」で停止する。Lean出力/新Rust定理は0。
関数値・キャスト・間接呼出しのPure意味論、receiver/ABI/存続期間、reference→NonNull、
IC/Wasm/ledger外部runtimeとの対応は引き続き未証明。

再現: `bash scripts/build-fn-item-candidate.sh`、`bash scripts/diagnose-ledger-fn-item.sh`。
固定範囲: `tool-patches/fn-item-profile.json`。前fn-transmute候補と固定Mainを保持する。

### Retained function-item Pure coercion (experimental)

`CastRustFnItem(src,dst)`をPure ASTへ追加した。両端の型は通常のtyであり、
型代入・visitorが両方に届く。方向も保持する。symbolic変換では固定crateの
`fn_item_reification_signature_matches`による正確な署名照合、Lean backend、
単一operandのPure型一致を要求する。実際の型変換が成功した場合に限り、
このinfallible item-to-pointer coercionを演算として構成する。両extractorは
verified callable extraction未対応として明示的に拒否する。

Native回帰ではsynthetic operatorの両端の型変数への代入と逆方向の区別を確認した。
これは実キャストの意味論証明ではない。実17本体では2つのcastが既存の署名照合を通り、
以前の「TODO: function casts」を越える。しかし自由Self型を含むFnPtrのPure表現が
未対応で、SymbolicToPureTypes:40にて停止する。従ってこの実入力でPure coercion値を
生成できたとはしない。Args.newの参照→NonNull transmuteも未対応のまま。
Lean出力/新Rust定理は0。receiver/ABI/lifetime/外部実行の対応は引き続き未証明。

再現: `bash scripts/build-fn-cast-candidate.sh`、`bash scripts/diagnose-ledger-fn-cast.sh`。
固定範囲: `tool-patches/fn-cast-profile.json`。前fn-item候補と固定Mainを保持する。

### Open function-pointer type templates (experimental)

自由型変数を含むRust ABI/nonvariadic FnPtrについて、型スロットを所有するmetadataと
通常のPure型引数を導入した。signatureの局所region binder、ABI、安全性、引数/戻り値の
構造は保持する。metadata中のBound型変数(depth 0)はtemplate_type_slotsのスロットを指し、
元Rustの型binderを意味しない。元のBound型変数を含むシグネチャは受け入れない。
自由型のない既存closed markerは変更しない。既存のclosed-only拒否テストはclosed translatorを
明示的に呼び、default translatorがopen templateを受け入れることと区別する。

検査には既存closed-shape guardを使う。自由型をTNeverへ置くのはshape検査用probeだけであり、
保存/抽出するsignatureはそのprobeではない。metadata中の型スロットへ抽象化したsignatureを
capturesで再構成すると元のsignatureになる。自由region、const/trait引数、nested FnPtr、
array/slice/dyn等の従来未対応形状は引き続き拒否する。callable extractionは明示的に拒否する。

実16 open signatureで完全再構成、Pureのcapture代入とraw再構成の一致を確認した。
64 injectiveな自由変数ID付替えでmetadata不変を確認し、複数スロットの順序/繰返しを区別した。
192範囲外の変形を拒否した。いずれも有限のNative回帰検証であり、全入力変換定理ではない。
実17本体ではitem-to-pointer branchの型変換とoperand照合が通り、
次はSymbolicToPureExpressions:624のPure transmute未対応で停止する。
Args.newの参照→NonNullも未対応のまま。Lean出力/新Rust対応定理は0。
既存のsynthetic operator traversalテストの表示はその単独テストを指し、
この実ソースpipelineの到達状況とは区別する。generic置換後のcanonical型同値性、
実callable値とABI/receiver/provenance/lifetime、IC/ledger外部runtimeとの対応は未証明。

再現: `bash scripts/build-fn-template-candidate.sh`、`bash scripts/diagnose-ledger-fn-template.sh`。
固定範囲: `tool-patches/fn-template-profile.json`。前fn-cast候補・固定Mainを保持する。

### Typed Pure function-pointer transmute (experimental)

`CastRustFnPtrTransmute(src,dst)`をPure ASTへ追加した。item-to-pointer coercionと演算を
区別し、両端の型を通常のPure tyとして保持する。元の自由型captureには型代入が届く。
symbolic変換は既存の`fnptr_transmute_representation_supported`を再利用し、Lean backend、
両端のRust ABI/nonvariadic/有効rty、unsafe target、同じarity/outputに加え、
単一operandのPure型一致を要求する。representation-onlyの演算で、call ABIや
receiver/provenance/lifetimeの正しさをこの条件から導かない。callable extractorは明示的に拒否する。

Native検証では実2 transmuteからPure演算を構成し、両端の正確な型と演算種別を確認した。
元のopen pointer型captureへの代入が届き、closed targetのmetadataが変わらないことも確認した。
22 source/target変形と3他backendを、型変換callbackへ入る前に拒否した。
finite回帰検証であり、全入力Rust/Pure変換定理や実関数呼出しの証明ではない。

実17本体ではPure transmute未対応の停止を越え、trait function-itemのVaFnDef値変換にある
SymbolicToPureExpressions:1681のUnimplementedで停止する。Args.newの参照→NonNullも未対応。
従って関数本体全体のPure/Lean出力はまだなく、Lean出力/新Rust対応定理は0。
次はtrait function-item値のidentityとcaptureを保持する表現・型検査・抽出義務が必要。
CFI、実call ABI/receiver、IC/Wasm/ledger外部runtimeの対応は引き続き未証明。

再現: `bash scripts/build-fn-pure-transmute-candidate.sh`、
`bash scripts/diagnose-ledger-fn-pure-transmute.sh`。
固定範囲: `tool-patches/fn-pure-transmute-profile.json`。前fn-template候補・固定Mainを保持する。

### Retained trait function-item values (experimental)

`RustTraitFnItemValue` qualifierは、元関数型のtrait/method IDと局所region binderを
閉じたmetadataに保持し、値側の消去済みregion引数を別フィールドに保持する。
Self型とtraitの証拠は通常のPure generic argumentsで、値と型の双方へ代入が届く。
通常のFun値の既存経路は維持する。callable extractionは明示的に拒否する。

factoryは元TFnDefの対応範囲、値kind/型引数の完全一致、既存method署名の存在、
値region引数の通常消去結果、期待Pure型の一致を検査する。Pure qualifierのsanity checkは
metadataの局所寿命scope、method存在、型とqualifierの一致、単一Self型と単一trait証拠、
trait IDとSelf型の一致を検査する。metadataには自由型/clause変数を隠さず、閉じたID/寿命のみ
opaqueとして保持する。依存順序は既存のtrait method値と同様に扱う。

実2 CFnDef定数の検証では、既存`PrePasses.erase_body_regions`を実際に適用した値を使った。
この前処理は関数値のRBody/RVar/RStaticをRErasedにし、型内のfunction binderは保持する。
Charonのregion-variable代入だけではRBodyが消去されず、同じ処理として代用できない。
テスト入力のJSON書換えや独自erasure shimは使っていない。

実2値のidentity、型binderと値寿命の区別、Self/trait証拠の代入後のsanity一致を確認した。
12不正値、18不正qualifier型、期待型不一致、3他backendを拒否した。
未知method、壊れたscope、Self/trait証拠の食い違いは、qualifierと見かけの型を同じにしても拒否する。
これは有限回帰検証であり、全入力変換やRustの関数値意味論の証明ではない。
trait証拠の完全な環境型付け/実call ABI/receiver/provenance/lifetimeは未証明。

実17本体ではdebug/displayの2 constructorが以前のVaFnDef停止を越え、処理状況は16/17となった。
残る停止はfmt/mod.rs:734のArguments.newにある参照→NonNull transmute(InterpExpressions:1047)。
全体のLean出力/新Rust対応定理は0。全抽出・callable意味論・IC/ledger外部runtimeの対応は未完了。

再現: `bash scripts/build-fn-item-value-candidate.sh`、`bash scripts/diagnose-ledger-fn-item-value.sh`。
固定範囲: `tool-patches/fn-item-value-profile.json`。前fn-pure-transmute候補・固定Mainを保持する。

### NonNull source layout diagnostic (auxiliary)

従来17本体の入力ではNonNull<T>はForeign/Opaque、layout=[]だった。
補助抽出で`--include core::ptr::non_null::NonNull`を1つ追加すると、固定coreの実定義が
`#[repr(transparent)]`な単一pointerフィールドとして得られた。フィールドは
`TPattern(TRawPtr(TVar Free0, Shared), NotNull)`。layoutのsize/alignは、このフィールドに
対するSizeOf/AlignOfの記号的保証であり、generic Tについて数値8などのchosen値はない。
これを既知の数値レイアウトへ置換せず、実レイアウト情報として保持する。

診断は基準/追加includeの双方を新規抽出し、17 clean bodiesと、include追加・出力先以外の
全設定一致を確認した。borrowck/typecheck/normalization/body lifetime/panic/layout計算の設定は
維持し、JSON入力の書換えは行わない。12 source-schema変形をPython診断で拒否した。
この診断はNative classifierの検証やLeanでのレイアウト定理ではない。
追加includeを既存Aeneas候補の入力ゲートへ採用したわけではなく、以前の16/17候補を保持する。

Arguments.newの実ソースと同じarray reference→NonNull<T>の式を使うprivate probeを追加した。
byte配列、非dereferenceのゼロ長配列、ZST要素(非ゼロ/ゼロ長)、64-aligned要素、
shared-reference payloadの5観測は固定Rust/Miriで通った。アドレス、サイズ/alignment、
存続中の非空arrayの先頭readを確認した。固定core自体の抽出証明ではなく有限観測に留まる。
ゼロ長u8配列のpointer.as_ref()はMiri専用負例でinvalid &u8として拒否された。
pointerを保持することだけではreadable elementの存在を導けない。負例はnative実行しない。

次はNotNull/raw-pointerの型・値表現と、参照→NonNull変換のallocation/offset/metadata/
provenance/元borrowの存続期間の対応が必要。fmt::Argumentsのunsafe caller invariants、
IC/Wasm/ledger外部挙動、全入力Rust対応は引き続き未証明。Lean出力/新Rust定理は0。

再現: `bash scripts/diagnose-ledger-nonnull-layout.sh`。
固定範囲: `tool-patches/nonnull-layout-profile.json`。最新追加includeのLLBCは
`.local/proof-tools/ledger-nonnull-layout/LedgerNonNullLayoutSource.llbc`に保存する。
profileのlast_source_artifactは直近snapshotのhashであり、次回の一時出力先による差分は
プログラムの書換えと区別する。

### Retained non-null raw-pointer types (experimental)

追加NonNull includeを厳密な入力profileとして受け入れ、実フィールドの
`TPattern(TRawPtr(T,kind),NotNull)`をPureの`TRustNotNullPtr kind`として保持する。
参照先の型Tは通常のgeneric引数で、型代入が届く。non-null制約とmutabilityを区別し、
pointer value extractionは明示的に拒否する。bare raw pointerや他patternの既存経路は維持する。
これはtype markerであり、非nullの実値を生成/証明したわけではない。

NonNull定義の展開で、参照先の型にある借用をpointer値の格納借用と誤分類する問題が現れた。
NotNull raw-pointer patternに限り格納値の借用を追加せず、既存の外側/隣接の借用flagsを保持する。
型のfree regionは別visitorで追跡を維持する。無視した参照先が実値として有効であるとは導かない。
実定義のprojection比較も未対応だったため、同じNotNull条件/同じpointer kindを要求し、
元のregion比較を完全なpointee型へ再帰させた。型の寿命metadataや比較callbackの結果を消さない。

Native検証では実pointer fieldのcapture代入、non-null/mutability区別、6範囲外pattern、
3他backendの拒否を確認した。格納pointee借用の分類、shared/mutableの外側/隣接context flagsと
自由region inventoryを確認し、構造比較はpointeeとcallback結果を保持、kind不一致をcallback前に拒否した。
7設定変形をNativeで、5設定変形を実CLIで拒否した。有限検証であり全入力変換定理ではない。

実NonNull定義を含む17本体は16/17まで進む。型展開で生じたgreedy expansion/region比較の停止を
解消し、残る停止はArguments.newの参照→NonNull transmute(InterpExpressions:1047)。
追加定義前の候補と元17入力の16/17も再現した。初期型候補の8/17 greedy expansion、
分類修正後の4/17 projection比較停止はこの候補の完成状態では残らない。
Lean出力/新Rust対応定理は0。実pointer生成・allocation/offset/metadata・provenance・
元borrowの存続期間、generic canonical型同値性、IC/ledger外部実行の対応は未証明。

再現: `bash scripts/build-notnull-pattern-candidate.sh`、`bash scripts/diagnose-ledger-notnull-pattern.sh`。
固定範囲: `tool-patches/notnull-pattern-profile.json`。前fn-item-value候補・固定Mainを保持する。

### 配列参照→NonNullの表現照合と未証明の借用対応

`tool-patches/aeneas-array-nonnull-representation.patch` は固定
`aarch64-apple-darwin` の実NonNull宣言を照合する隔離候補。
共有array参照の要素型とNonNullの型引数、単一のcovariant型parameter、
非nullconst-pointer field、透明表現、記号的SizeOf/AlignOf保証を検査する。
元の借用operandの内容に代入するポインタ値の意味論は実装していない。

実Arguments.newの2変換は既存の関数単位body-region消去前処理後に照合できた。
元のArgument型は変換元と変換先に異なるRBody IDを持つため、その消去前後の寿命対応は
未証明である。消去後の型一致を元Rust借用寿命の同値性とみなさない。
46型・宣言・layout変形の拒否、empty arrayの表現照合、Rust/Miri各5観測を確認した。
いずれも有限Native/実行検証であり全入力の証明ではない。

実ソース17本体は16/17で、Arguments.newの最初の変換を
`Array-reference to NonNull requires verified pointer region/provenance correspondence`
として明示的に拒否する。abort-on-errorの既存表示はFailure/Uncaught exceptionを含むが、
この義務のErrors.craiseによる停止を検証し、探索時に生じたty_regionsの内部Failureを
完成状態に残していない。Lean出力/新Rust対応定理は0。

再現: `bash scripts/build-array-nonnull-candidate.sh`、
`bash scripts/diagnose-ledger-array-nonnull.sh`。
固定範囲: `tool-patches/array-nonnull-profile.json`。
次の義務は共有borrow→raw pointerのallocation/address/provenance/region対応である。

### 実共有borrowのSymbolic pointee-region保持

`tool-patches/aeneas-array-nonnull-symbolic-regions.patch` は前候補の実layoutガード後、
`VSharedBorrow` のborrow IDで実loanを検索する。その内容がSymbolic arrayであり、
完全な寿命付きarray型がrtyを満たし、型消去後にoperandのarray型と完全一致する場合だけ
NonNullの型引数へ完全なpointee型を保持する。新規region IDやloanを捏造しない。
既存のunary-op合成は共有borrow operandとtransmute操作を保持する。
この処理はSymbolic型の対応であり実Rustポインタのメモリ意味論の証明ではない。

実2変換の型を用いるNative対照では保持後の完全なpointee型、消去後のdestination型との一致、
free region inventory、終了regionとの交差判定を確認した。13不適合loan型を拒否した。
既存body-region消去を使う点、元Rust lifetime対応の定理が未追加である点は変わらない。
新鮮な実Arguments.newのSymbolic評価は完了してPure変換へ進み、そこで
`Retained array-reference to NonNull requires verified pointer value extraction` と明示的に停止する。
誤ったfunction-pointer診断や探索時のerased/body region内部Failureは残らない。
実ソース16/17・Lean出力/新Rust対応定理0、Rust/Miri各5観測が通る。

再現: `bash scripts/build-array-nonnull-regions-candidate.sh`、
`bash scripts/diagnose-ledger-array-nonnull-regions.sh`。
固定範囲: `tool-patches/array-nonnull-regions-profile.json`。
前array-nonnull候補と固定Mainは保持する。次の義務はアドレス・allocation・provenanceを保持する
Pure pointer値と実Rust/Lean対応であり、NonNull型のlayoutだけでdereferenceを正当化しない。

### 実17本体のPure変換と共有borrow由来の保持

`tool-patches/aeneas-array-nonnull-pure-origin.patch` はPure演算
`CastRustSharedArrayNonNull(origin, src, dst)` を追加する隔離候補。
originは実call contextで検索したborrow ID、shared borrow ID、loanのSymbolic IDを保持する。
これらは抽出器内の識別子であり、Rustのallocation IDや実アドレスとはみなさない。
operandの共有borrow形状・型、loanのSymbolic形状・消去型、完全なdestination region型を
検査してからPure型を変換する。両端点は通常の型代入visitorで辿れる。

Nativeでは実2変換の型による21不正origin/loan/destination対照と3他backendを型変換前に拒否した。
source arrayのconst capture代入、別のoriginの区別、同じloanに対するshared reborrowの保持を確認した。
さらに実クレートに標準47前処理とPure変換・後処理を実行し、17本体と実Arguments.new内の
2演算が後処理後も残り、実際の非負borrow/shared/loan IDを持つことを直接検査した。
単体対照に使う仮のIDだけで実パイプライン通過を判断していない。

新鮮な実CLIもPure変換17/17・後処理17/17を通る。Lean抽出はNonNull型のpointer fieldで
`Retained non-null pointer type requires verified pointer value extraction` と明示的に停止する。
型抽出が先なので演算抽出のaddress/provenance拒否へはまだ達しない。
途中のTypes.leanが1つ作られ、NonNullのrust_type属性で終わる不完全なファイルであることを
検査して一時領域に隔離する。完成したLean成果物/olean/新Rust対応定理は0。
旧候補の「Leanファイル自体が0」とこの候補の状態を混同しない。

Rust/Miri各7有限観測が通る。追加2観測は同じ内容の別非空arrayが別アドレスを持つこと、
同じarrayの共有再借用が同じアドレスを持つことを確認する。originの識別子の違いから
アドレス不一致を推論しない。これはRust/Leanメモリ対応の全入力定理ではない。

再現: `bash scripts/build-array-nonnull-pure-candidate.sh`、
`bash scripts/diagnose-ledger-array-nonnull-pure.sh`。
固定範囲: `tool-patches/array-nonnull-pure-profile.json`。
前候補と固定Mainを保持する。次の義務はRust実allocation/address/provenanceとPure pointer値を
関係づけるメモリ意味論、そしてそのLean抽出対応であり、17/17を証明完了と数えない。

### ポインタ範囲のLeanモデル義務（実Rust対応は未証明）

`operator-lean/SharedArrayPointerFootprint.lean` は、残るpointer意味論に必要な条件を
14のMODEL定理として形式化する。固定Rust `ptr/mod.rs` の最小保証に沿い、
非null、allocation/provenance、spatial lower/upper bounds、temporal live、read/write権限を分離した。
`ReadFootprint` は必要な範囲・権限条件だけであり、初期化・型のvalid value・aliasing・
concurrencyやRustポインタ→参照変換の安全性を主張しない。
固定Rust文書自身も正確なvalidity/aliasing規則は未決定と述べている。

保持モデルは任意の要素数・要素byte sizeについてaddress/offset/provenance、
read footprint、alignment、write permissionを保持する。
正の要素数とarray全体のfootprintから先頭要素の必要footprintを導き、
空extentの正サイズreadを拒否する。zero-byteアクセスと正サイズアクセスを分離し、
temporal権限を終了しても非null carrierは保持できるが正サイズread条件は満たさないことを示す。
これらはモデル定義の定理であり、実Rust transmuteをこの保持モデルへ対応づける定理ではない。

同じborrow/loanのshared aliasを解決する規則は`OriginResolution`の明示的パラメータ義務であり、
実ctx・allocation・権限からそのadapterを構成する証明は残る。
同じaddressでもprovenanceが異なる場合（失効した旧allocationと別allocation）、
同じloan Symbolic IDでも別borrow/allocationへ解決できる場合をLeanで反例として証明した。
Symbolic IDはcontentを表し、allocation IDとはみなさない。これは今後のpointer抽出で
addressだけ・Symbolic IDだけへ同一視する近道を防ぐためのMODEL反例である。

`SharedArrayPointerFootprintAudit` が14定理とnamespace全体の公理依存を検査する。
warningAsError、leanchecker、axiom/sorry/native_decide混入の拒否、非nullだけで正サイズreadを
許す誤証明の拒否を確認した。新しい実Rust対応定理は0で、前Pure17/17候補の
NonNull型抽出拒否は変更しない。

再現: `bash scripts/diagnose-shared-array-pointer-footprint.sh`。
固定範囲: `tool-patches/shared-array-pointer-footprint-profile.json`。
次の実ソース義務は借用ctxとRust allocation/address/provenanceを関係づけるadapterと
その履歴・alias・権限保存、LLVM/Rust transmute意味論との対応である。

### 外部ledger artifactと抽出版の接続義務

公式Rust1.93.1/linux-amd64ビルド環境でwasm32対象のledgerを再現ビルドし、
同梱Wasmとの全バイト一致を確認した。`../external/ledger-reproduction-profile.json`に固定入力と結果を記録する。
これはsource artifactの対応証拠でありRust/LLVM意味論の形式証明ではない。
既存のnightly2026-09-17/aarch64-apple-darwin抽出とは標準ライブラリ・target/layoutが異なる。
特に公式Rust1.93.1のNonNullはscalar-valid-range属性付き通常raw pointer fieldで、
nightlyのNotNull pattern fieldとは表現が違う。既存17本体/Pure型保持/14 pointer MODEL定理を
公式ledger Wasmの全入力対応とみなさず、stdlib/targetの対応義務を残す。

公式Rust1.93.1/wasm32のstandalone Duration probe MIRを得てnightly/aarch64と比較した。
`duration_nanos`の全MIR本体は一致する一方、releaseのTryFromIntError payloadはunit、
nightlyはPosOverflowで、unwrap制御フローも異なる。
`../external/ledger-release-mir-profile.json`にflags・scope・hashを固定した。
式のテキスト一致を全入力意味論やpanic表示の対応とみなさず、nightly TypedError証明の
release接続義務を残す。新Rust/Lean対応定理は未追加。

公式Rust1.93.1・wasm32の実 `ledger_core` コンパイル引数によるTimeStamp MIRも取得済み（`../external/ledger-core-release-mir-profile.json`）。全依存と公式設定を維持し、出力だけをMIRに変更した。実add/subでは標準ライブラリ4段階の呼び出しが残るため、先の追加MIR最適化レベル3のプローブと区別する。実ソースMIRを得たことは、既存nightly抽出や条件付きLean定理との全入力対応証明を意味しない。

公式実TimeStamp MIRのローカル番号から生成した限定呼び出し列について、11件の補題・条件付き定理を検証した（`../external/release-timestamp-sequence-profile.json`）。`CallContracts`を明示的仮定として、全モデル有効Duration/U64 timestampで値またはpanicの観測が一致する。限定IRへのMIR抽象化、Rust1.93.1標準ライブラリ契約、layout/provenance、panic/trap、LLVM/Wasm/IC対応は未証明のまま。`scripts/verify-release-timestamp-sequence.sh` の厳格Lean・公理監査・leanchecker・4否定対照は成功した。

公式実MIRから生成した5ブロックの加減算CFGについて、限定CFG意味論から前回の呼び出し列への一般的意味論保存を証明した（`../external/release-timestamp-cfg-profile.json`）。新11定理の厳格Lean・公理監査・leanchecker・6否定対照が成功。閉じたのは限定ブロックAST→列の変換義務であり、実Rust MIR→ブロックAST、Storage/ref/move/unwind、stdlib契約、LLVM/Wasm/ICの義務は未証明。

公式TimeStamp実MIRのStorageLive/Deadを省略しない20イベント列から、16件のローカル寿命/初期化MODEL定理を検証した（`../external/release-timestamp-lifetime-profile.json`）。Move消費の有無両方で全列・全接頭辞のチェックが通る。固定Rust1.93.1一次ソースのMove意味論の未確定部分を確定したとは扱わない。実型/参照/provenance、実行トレースと抽出の対応、panic commit、stdlib/LLVM/Wasm/ICは未証明。

ローカル寿命checkerの一般健全性と実20イベント列/全接頭辞への適用6定理も検証した（`../external/release-timestamp-lifetime-soundness-profile.json`）。checker成功から、各宣言された読出しのlive/初期化・各値書込み先のliveを導ける。2新ファイルの厳格Lean、公理監査、leanchecker、5否定対照が成功。Rust実行対応・参照/provenance・メモリ安全性・stdlib/LLVM/Wasm/IC対応を閉じたとは扱わない。

### 実JUMPDESTの最小抽出と寿命消去の阻害要因

`tool-patches/revm-jumpdest-profile.json` は `instructions/control.rs` の実JUMPDEST（空の関数本体）を2経路で新規抽出した診断である。汎用経路は本番evm-core manifest/root Cargo.lockを使用し、使用しないInterpreter内部を明示的にopaqueとする。命令本体は改変せず、LLBCはunit戻り先0の初期化2回・フレームStorage・returnのみ。InstructionContextのinterpreter/host両参照はMut、同じFree0寿命と1個の宣言寿命binderを保持し、MemoryTr GATも保持する。Aeneasは実本体1/1をPureへ翻訳するが、GAT宣言で厳格拒否する（SymbolicToPure.ml224）。受理されたLeanファイルはない。

完全単相化経路は既存の正確なJUMPDEST呼び出しコーパスのEthInterpreter/DummyHost wrapperを別のlocked依存グラフで使用する。実命令とwrapperの2本を抽出でき、trait宣言は0になる。一方、InstructionContextの寿命binderは0、両Mut参照の寿命がErasedになり、Aeneas型解析が本体翻訳前に拒否する（TypesAnalysis.ml229）。Erasedを無視したり仮の寿命を挿入したり、LLBCを書き換えて通したりはしていない。追加のoptimized MIR/最適化フラグ指定でも同じErased拒否が残った。指定フラグが本番構成やコンパイラ意味論に同値であるとは扱わない。

`diagnose-revm-jumpdest.sh` は両経路を毎回新規抽出し、実関数のunit-only文形、全入力参照種別/binder/GAT情報、既知の厳格拒否とLean出力なしを確認する。`audit_jumpdest_llbc.py` は引数への書込み、非空unit aggregate、Context参照種別の変更というコピー証跡の改変3件ずつを拒否する。診断ゲートは成功したが、新しい命令定理は0件。JUMPDESTディスパッチのbase gas/PC更新、Rust/LLVM/Wasm実行対応、全EVM命令の意味論は未証明。型/trait引数を特殊化しながら元の借用寿命を保つか、汎用GAT宣言を正しくサポートすることが次の抽出義務である。

### GATを含まない寿命単相化の回帰例

`mono-lifetime-rust` は依存なしの実Rust最小例である。`Same` の2参照は1宣言寿命、`Split` の2参照は別々の2宣言寿命を持ち、左右の参照を返す関数も含む。汎用Charon抽出は宣言binder・参照のFree0/Free1・戻り先の寿命を保ち、固定Aeneasで4/4関数を厳格Lean翻訳できた。生成物を変更せず `u256-lean/MonoLifetimeGenerated.lean` に保存し、全生成U32入力でno-op結果、左右の返値/更新関数/更新しない値snapshot、各更新関数が選んだ側だけ変えることの8定理を検証した。別寿命の戻し先snapshotが値として等しくても、物理アドレスやallocationが等しいとは意味しない。

同じ実ソースの完全単相化は宣言寿命を0個にし、可変参照の寿命を両方Erasedへ消す。GATを含まない例でもAeneasはTypesAnalysis.ml229で厳格拒否し、Leanを生成しない。固定Charon一次ソースの `translate_crate.rs` / `item_ref.rs` に寿命消去とADT引数を空にする経路を確認した。既存の部分単相化は型引数内の寿命も捕捉するため、フィールドごとに新しい寿命を足したり、全参照を同じ寿命にしたりして修復できるとは扱わない。ツール本体は今回変更していない。

`verify-mono-lifetime-fixture.sh` は固定SHA、新規2経路抽出、生成Leanのbyte一致、LLBC寿命関係の改変4件、公理監査、leanchecker、公理/sorry/native_decideの3拒否、左右を取り違えた更新関数の偽定理2拒否を再現する。`tool-patches/mono-lifetime-profile.json` に範囲を固定した。新EVM命令定理は0件。元の宣言寿命と型引数内の寿命を保つ特殊化、実revm型/GAT対応、Rustのheap/provenanceとコンパイラ/LLVM/Wasm/IC実行対応は未証明である。

実JUMPDESTの診断を既存 `--monomorphize-mut=all` による部分単相化にも拡張した。固定Charonの一次ソースによれば、これは可変参照を含む型引数の形を特殊化する処理であり、任意の型/trait引数をすべて具体化する機能ではない。新規抽出でInstructionContextのFree0と1宣言寿命を保つことを確認したが、MemoryTrのGATは残り、Aeneasは同じGAT宣言を厳格拒否した。Lean出力はない。

実MemoryTrの `slice_len` はreceiver寿命に結び付いた `impl Deref<Target=[u8]>` を返す。LLBCの対応する `slice_len_ty` は1寿命binder、型のoutlives制約1個、型等値制約1個、既定型、Derefの含意節1個を保持する。AeneasのPure trait型宣言は関連型をID/nameのペアとして扱い、SymbolicToPureはbinderや含意節がある場合を拒否する。警告だけ抑える変更や寿命/制約を削る変更では実ソースとの対応を満たせない。診断はこのメタデータの消失5件も拒否するよう強化し、全3経路で19件のコピー証跡改変を拒否した。新命令定理は0件で、ツール本体の修正は未実施。

### 実JUMPDEST生成本体の全入力2定理

`verify-revm-jumpdest-experimental.sh` は本番evm-core manifest/root Cargo.lockで実 `instructions/control.rs::jumpdest` と実Interpreter/Gas/MemoryGas構造体を新規抽出する。既存Charonの `--remove-unused-clauses` を明示的に使うと、空の実命令本体では不要なInterpreterTypesの証明引数を除ける。元のLLBCにはMemoryTr GATの寿命・制約・既定型・Deref境界を保持するが、そのtrait宣言は抽出依存グラフから外れ、厳格Aeneas翻訳が通る。同じ経路でInterpreterをopaqueにした診断は型公理を生成するため、受理しない。実Interpreterの8フィールド、実Gasの4フィールド、MemoryGasの2フィールドを透明な構造体として抽出すると、公理を使わない生成物になる。LLBC/生成Leanの書き換えはしない。

`u256-lean/RevmJumpdestGenerated.lean` を生成物のbyteコピーとして保存し、`RevmJumpdestCorrespondence.lean` で全生成コンポーネント型とコンテキスト入力について、命令本体がコンテキスト値をそのまま返すこと、および任意の値観測が保存されることの2定理を証明した。これは実JUMPDEST生成本体の性質であり、fixtureの恒等関数への置換ではない。再抽出byte一致、型引数/寿命/outlives/GAT/フィールド監査、LLBCコピー証跡の改変12件、厳格Lean、公理監査、leanchecker、公理/sorry/native_decideの3拒否、gas書込み/失敗戻りの偽定理2拒否をゲートで検証する。

範囲は `tool-patches/revm-jumpdest-transparent-profile.json` に固定する。Charonの一次ソースは、未使用trait節の除去がunsafe操作の型前提を消す可能性を明記している。今回の実LLBC本体はStorage/unit初期化/returnだけと監査するが、除去処理の意味論保存とRust有効状態・型領域への対応は未証明である。全命令への一括適用はしない。元のRust heap/参照/provenance、Charon/Aeneasとコンパイラの意味論保存、命令ディスパッチのbase gas/PC更新、LLVM/Wasm/IC/ledger全挙動も未証明である。全Rust/EVM命令対応証明の完了数を2へ増やしたとは扱わない。

### 実stepディスパッチの抽出と関数ポインタ阻害要因

`diagnose-revm-step-dispatch.sh` は本番evm-core manifest/root Cargo.lockから、実 `Interpreter::step` / `Instruction::static_gas` / `Gas::record_cost_unsafe` / `Interpreter::halt_oog` / `Instruction::execute` / `Interpreter::halt` の6関数を透明な実本体として抽出する。unused-clause除去は明示し、その意味論保存は未証明である。実LLBCはopcode読出し→relative_jump(1)→配列slice/unchecked参照→static gas→wrapping gas subtractionを保ち、OOG boolがfalseの分岐だけでexecuteしてreturnする。不足側のhalt_oogはspend_allを呼んでからhaltする。ソースのspend_allはremainingを0に設定するが、今回の抽出経路でこの関数とnew_halt、bytecode trait、標準ライブラリ操作の本体は依然opaqueであり、全実行効果の証明とは扱わない。

実Instruction.fn_は安全なRust ABI・可変長引数なし・寿命binder1個・Bound(0,0)寿命のInstructionContext入力1個・unit返値の関数ポインタで、実executeは保存した関数ポインタを間接呼出しする。固定AeneasはSymbolicToPureTypes.ml190でこの関数型を厳格拒否する。Lean成果物は0件。呼出しを固定JUMPDESTへ置き換えたり、FnPtrを単なるContext→unitへ写したりせず、可変参照のコンテキスト/host状態の戻し方とbinder/ABI/呼出し先意味論を保つ対応が必要である。

`audit_revm_step_llbc.py` は6関数の在庫、呼出し順、PC増分1、OOG分岐の極性、間接呼出し、関数型のABI/binderを監査し、コピー証跡の改変6件を拒否する。固定SHAと新規再抽出で厳格拒否/Leanなしを再現した。`tool-patches/revm-step-dispatch-profile.json` に範囲を固定する。ASTの監査を実制御フロー/実行意味論の証明とは扱わない。bytecodeの生ポインタread/offset/provenance/padding、配列参照安全性、実命令表/gas/PC対応、抽出/コンパイラ/Wasm/IC対応も未証明で、新しい実行定理は0件である。

実stepの公式 `-borrow-check` 経路も調べた。Pure翻訳を行わない基準バイナリはInstructionフィールドの関数値を新しいsymbolic valueへ展開する際にrty検査で拒否する（InterpUtils.ml151）。固定済みfn-region候補ではその検査を越えるがgreedy expansionで拒否し、fn-valueおよび最新array-nonnull-pure候補では実 `Instruction::execute` の `FnOpDynamic` まで進んで拒否する（InterpStatements.ml1254）。4候補のバイナリを変更せず、固定SHAと新規抽出の診断ゲートで各拒否箇所を再現した。全6関数の借用検査成功や間接呼出し対応済みとは扱わない。

一次ソースではconcrete/symbolic両評価器がFnOpDynamicを未対応として拒否する。symbolic_fun_call_inst.call_kindとSymbolicAst.call_id.Funは既知の関数/trait method識別子を使い、Pureの関数型分解も宣言別のeffect情報に依存する。実間接呼出しを扱うには、評価した関数値とその署名/effectを保持し、局所bound寿命を正しくinstantiateし、借用abstractionとbackward outputを保った上で、その値へのapplicationを生成する必要がある。型のArrow受理だけ、偽の宣言ID、固定JUMPDESTへの置換、forward unitだけの翻訳では、この実ソース対応義務を閉じられない。型翻訳と借用評価の双方が未対応であり、新定理は0件のままである。

間接呼出しの値の由来についても実LLBCを監査した。実executeはself.fn_（フィールド0）をlocal3へCopyし、元のctx（local2）をlocal4へMoveして、local3の関数値をlocal4で動的に呼び、unit結果をlocal0へ書く。保存フィールドと呼ぶ関数値のFnPtr型は一致し、H/WIRE/8コンポーネント型の束縛 `[1,0,2..9]` と局所Bound(0,0)寿命を保持する。入力Contextのinterpreter/hostは同じ宣言寿命Free0のMut参照である。コピーした証跡のフィールド取り違え、ctxの代わりにselfを渡す変更、別localをcalleeにする変更も拒否し、全9否定対照を新規抽出ゲートで検証した。この由来/型束縛の監査は、実メモリ・借用・Rust操作的意味論の証明ではない。関数値を保持した間接applicationと借用の戻し処理は依然未実装・未証明である。

### 間接呼出しのborrow-only分離候補

`aeneas-dynamic-borrow-call.patch` は固定fn-valueパッチ列に重ねる未採用候補である。公式 `-borrow-check` モードだけで、実FnOpDynamicのoperandを評価し、その関数値をevaluated_callbackに保持する。既知の関数宣言IDを作ったり、固定JUMPDESTへ置き換えたりしない。安全なRust ABI・非variadic・局所寿命binder1個・同じ局所寿命を持つADT Context入力1個・unit返値の範囲に限り、元の型/const引数を保持し、Bound(0,0)を署名パラメータへ移して既存のfresh-region/借用abstraction処理で具体化する。実step/static_gas/record_cost_unsafe/halt_oog/execute/haltの6透明本体を含む抽出単位は、この候補で借用検査に成功した。

`DynamicBorrowRegression.ml` は実Instruction.fn_署名から型環境・コンポーネント引数・寿命の保持を確認し、unsafe、variadic、binderなし、非unit返値、captured寿命、erased寿命の6署名をヘルパーで直接拒否する。コピーLLBCの改変は未整合な型やパーサ拒否により先に止まる場合があるため、これを直接ヘルパー拒否と混同しない。`verify-dynamic-borrow-candidate.sh` は固定SHA、新規実ソース抽出、実LLBC監査の9否定対照、ネイティブML署名回帰6否定対照、候補での実6本体borrow-check成功、Lean生成の厳格拒否/成果物なしを検証する。

`build-dynamic-borrow-candidate.sh` は分離ビルドと強制Charon MLテストを実行し、変更した基準ソースを復元する。基準バイナリと既存候補は置き換えない。`tool-patches/dynamic-borrow-profile.json` にソース/バイナリ/復元元SHAと範囲を固定した。今回の検査成功は未採用候補による借用検査であり、借用エンジンの健全性・実Rust対応の新しい定理ではない。concrete間接呼出し、captured寿命や任意署名、symbolic ASTに関数値を保持する生成処理、effect/backward型、Pure applicationと状態/hostの戻し処理、Rust heap/provenance・コンパイラ・EVMディスパッチ・Wasm/IC/ledger意味論は未証明である。候補でもLean生成はopen FnPtrで厳格拒否し、新しい実行定理は0件である。

### 間接呼出し署名の前向き・後向き型分解候補

`aeneas-dynamic-call-decomposition.patch` は前節の未採用borrow-only候補に重ねる分離候補である。既存の署名分解本体を、region groupごとのeffectとログ用の名称を明示的に受け取るヘルパーへ分離した。通常の宣言IDを使う既存APIは元のeffect検索を渡す。未知のcallbackへ仮の関数宣言IDを付けない。この変更だけではFnPtrフィールド翻訳や動的AST/Pure applicationは有効にならない。

`DynamicDecompositionRegression.ml` は実Instruction.fn_の署名を元の型/const環境と局所寿命を保ってinstantiateし、前向きContext入力、後向きContext出力、合成Result Context出力を確認する。未知のcallbackとして前向きの失敗/発散と後向きの発散を保持する設定で検査する。後向き関数が失敗しない扱いは既存の署名分解契約に従うもので、任意の外部callbackの意味論を証明したものではない。unsafe/variadic/binderなし/非unit/captured/erased署名の6直接否定対照も残す。実透明6関数の通常署名について、同じ候補内の既存APIと新ヘルパーの分解結果が一致する。これはAPI配線の回帰検査であり、未変更基準バイナリとの独立比較や抽出器の意味論保存証明ではない。

`build-dynamic-decomposition-candidate.sh` は分離ビルド・強制Charon MLテスト・基準ソース復元を行う。`verify-dynamic-decomposition-candidate.sh` は固定SHA、新規実step抽出、LLBC改変9件、上記ML検査、実6本体の候補borrow-check成功、Leanの厳格拒否と成果物なしを検証する。範囲は `tool-patches/dynamic-decomposition-profile.json` に固定した。新しい実行定理は0件。実関数値の動的application、寿命/effectを含むFnPtrフィールド型、interpreter/host状態の戻し処理、未知のcalleeの意味論、実heap/provenance・抽出器/コンパイラ・全EVM・Wasm/IC/ledger対応は未証明である。

### 実executeの動的呼出しを保持するsymbolic AST候補

`aeneas-dynamic-symbolic-call.patch` は署名分解候補に重ねる未採用の分離候補である。SymbolicAstにDynamic呼出し識別子と評価済みcallbackのtvalueを保存するフィールドを追加し、既存の呼出し合成へ実operandの関数値を渡す。識別子は呼出しinstanceのIDで、仮の関数宣言IDや固定JUMPDESTではない。通常呼出しではcallbackフィールドはNoneのままになる。元のContext引数、具体化した署名、generics、借用abstraction、destとplace情報を保持する。

`DynamicSymbolicRegression.ml` は元のprepasses後の実Instruction::execute本体をsynthesize=trueで記号実行する。生成ASTに動的呼出しが1件だけあり、callbackがsymbolic FnPtr、引数が元のcallbackのContext型、未具体化/具体化署名が存在することを確認する。各借用abstractionは同じ動的call instanceへ結び付き、入出力を持つcontinuationを保持する。これは実ASTの構造検査であり、Rust heap/物理借用/外部host意味論の証明ではない。前候補の実callback型分解、通常6署名のAPI配線一致、6件の直接署名拒否検査も継続する。

`build-dynamic-symbolic-candidate.sh` は分離ビルド・強制Charon MLテスト後に、追加したAST/合成/表示ファイルを含め基準ソースを復元する。`verify-dynamic-symbolic-candidate.sh` は固定SHA・新規実step再抽出・実execute AST検査・実6本体の候補borrow-check・9件のLLBC改変拒否・Lean厳格拒否と成果物なしを検証する。`tool-patches/dynamic-symbolic-profile.json` に範囲を固定する。全体Lean翻訳はopen FnPtrフィールドで先に拒否される。新設したDynamic Pure applicationの明示的拒否分岐はコンパイルされるが、その全体ゲートで到達したとは扱わない。

新しい実行定理は0件。Pureの関数値型/applicationとeffect/backward closure、実interpreter/host状態の返却対応、concrete間接評価器、任意callback/寿命capture/ABI、symbolic評価器の健全性、Rust/extractor/compiler/全EVM/LLVM/Wasm/IC/ledger対応は未証明である。

### 動的Pure applicationの分離候補

`aeneas-dynamic-pure-application.patch` は前節のAST候補に重ねる未採用候補である。通常呼出しと動的呼出しで既存の後向き変数生成・借用abstraction登録・返却pattern構築を共有する。動的呼出しでは宣言IDを使わず、前向きの失敗/発散、後向きの発散を明示的に渡す。最終applicationは保持したcallbackのtvalueをPure値へ翻訳し、その値へ元の引数を適用する。calleeの型を期待型へ上書きしない。引数型とResult/backward返却型から求めたArrow型に厳格一致することを要求し、合わなければ拒否する。既存の通常/単項/二項/nanoseconds演算は従来のqualified関数構築を保持する。

`DynamicApplicationRegression.ml` は実executeのASTから具体化済み署名を取得して型分解し、抽象Pure自由変数のcalleeとContext引数でapplicationヘルパーを検査する。calleeと引数が同じ値のまま使われ、Result Context返却型が保たれる。unit返却、Resultを落とした返却、Context入力をunitへ変えた3型を拒否する。これは実署名から導いた型に対するPureヘルパー検査であり、実Rustのcallback tvalueをそのArrowへ翻訳できたことや、calleeの動作・状態返却の意味論保存を証明したものではない。

`build-dynamic-application-candidate.sh` は分離ビルド・強制Charon MLテスト後に基準ソースを復元する。`verify-dynamic-application-candidate.sh` は固定SHA、新規実step抽出、実executeのAST保持、実6本体borrow-check、LLBC9件/署名6件/Pure型3件の計18否定対照、Lean厳格拒否と成果物なしを検証する。範囲は `tool-patches/dynamic-application-profile.json` に固定する。新しいapplication翻訳分岐はコンパイルされたが、全体翻訳は依然open FnPtrフィールドで先に拒否する。全分岐を実Rust→Pure→Leanで実行できたとは扱わない。

新しい実行定理は0件。Contextを参照できるFnPtr型翻訳、元のABI/generics/higher-ranked寿命との対応、実callback値翻訳と全体application経路、heap/host状態返却・記号評価器/抽出器/コンパイラの健全性、任意callback/concrete評価器/全EVM/LLVM/Wasm/IC/ledger対応は未証明である。

### Contextを使う実FnPtr型翻訳の分離候補

`aeneas-dynamic-fnptr-type.patch` は前節のapplication候補に重ねる未採用候補である。前向き型翻訳と型宣言のトップレベルopen FnPtrフィールドに、元のContext型を参照する翻訳を追加する。対象は安全なRust ABI・非variadic・局所寿命binder1個・その寿命のADT Context入力1個・unit返値である。入力内のFree型変数IDを収集してidentity引数を作り、既存の寿命具体化とeffect/backward署名分解からArrow型を求める。補助binderには合成した名称/invariant metadataを使うが、入力/出力の元の型引数IDは保持する。これは補助binderの健全性やhigher-ranked Rust型とPure Arrowの形式的対応を証明したものではない。非Free型変数、入れ子の関数/trait型、const/trait captureは拒否する。既存closed FnPtr marker経路はそのままで、型宣言の他のフィールドは元のsignature-type翻訳を保持する。

`DynamicFnptrRegression.ml` は実executeのcallback FnPtr翻訳が実署名から作ったContext→Result Context application型と一致すること、実Instruction.fn_のArrowとstatic_gasのU64が保持されることを検査する。unsafe/variadic/binderなし/非unit/captured/erasedの6型を追加で直接拒否する。実AST保持と前候補のPureヘルパー検査を継続するが、全体の実Rust→Pure動的applicationが完走したとは扱わない。

`build-dynamic-fnptr-candidate.sh` は分離ビルド・強制Charon MLテスト後に基準ソースを復元する。`verify-dynamic-fnptr-candidate.sh` は固定SHA・新規実step抽出・実6本体のborrow-check・型/AST検査・LLBC9件/署名6件/Pure型3件/FnPtr型6件の計24否定対照を検証する。全体Lean翻訳はFnPtrフィールド拒否を越え、実MemoryTrのGAT関連型をSymbolicToPure.ml231で厳格拒否し、成果物を生成しない。GATの寿命/境界/関連型を消したりLLBCを書き換えたりしていない。範囲は `tool-patches/dynamic-fnptr-profile.json` に固定する。

新しい実行定理は0件。MemoryTr GATの正しい表現、実execute/step application全経路と失敗/発散効果の伝播、helperのidentity binder・Rust寿命/ABI/genericsとArrow型の対応、任意callback/concrete評価器、heap/hostの状態返却・抽出器/コンパイラの健全性、全EVM/LLVM/Wasm/IC/ledger対応は未証明である。

### 未知callbackの効果と実execute生成本体の3定理

`aeneas-dynamic-call-effects.patch` は前節のFnPtr候補に重ねる未採用候補である。既存FunsAnalysisはFnOpDynamicをclosure処理へ委ねて無視していた。今回の関数値経路では既知のcallee宣言を参照しないため、未知の動的呼出しを失敗/発散可能として解析し、呼出し元にも伝播する。`DynamicEffectsRegression.ml` は実executeのcan_fail/can_divergeが両方trueであることを検査する。`build-dynamic-effects-candidate.sh` は分離ビルド・強制MLテスト後にFunsAnalysisを含め基準ソースを復元し、`verify-dynamic-effects-candidate.sh` は実step再抽出/24否定対照/6本体borrow-check/次のGAT拒否を再検証する。解析健全性の形式証明ではない。

同じ本番evm-core manifest/root Cargo.lockから実Instruction::executeだけをrootとして、実Instruction/Interpreter/Gasも透明に新規抽出した。明示的なremove-unused-clausesにより、このrootで不要なMemoryTr証明引数は依存グラフから外れる。元のLLBCにはMemoryTr GATの寿命binder/outlives/型制約/既定型/Deref境界を保持する。実step全体のGAT拒否を解決したとは扱わず、このオプションの意味論保存も未証明である。特にexecuteは未知callbackを呼ぶため、その実装やunsafe型前提まで不要であるとは推論しない。

未採用effects候補で実executeを厳格Lean生成でき、無改変byteコピーを `u256-lean/RevmExecuteGenerated.lean` に保存した。Instruction.fn_はContext→Result Contextで、生成本体はself.fn_ ctxである。`RevmExecuteCorrespondence.lean` の3定理は、全生成コンポーネント型・任意のLean callbackを持つInstruction・Context入力について、本体がcallback適用に一致すること、任意のResult結果との一致がcallback結果との一致と同値であること、本体の結果がstatic_gasフィールドに依存しないことを証明する。全Result結果には成功時のinterpreter/hostを含むContextと失敗値を含むが、Lean callbackは実外部Rust callbackの意味論そのものではない。Rust発散や物理参照/provenanceの形式的対応も未証明である。実ディスパッチがstatic gasを課金しないとは意味しない。

`audit_revm_execute_llbc.py` は実callbackのself.fn_由来、元ctxのMove、宣言フィールドにselfの元型引数を代入した評価済みFnPtr型、寿命/ABI、全本体操作列/unwind、実透明構造体、残存GAT情報を監査し、コピーした証跡の11変更を拒否する。`verify-revm-execute-dynamic.sh` は固定SHA、新規抽出、生成byte一致、厳格Lean3ファイル、公理監査、leanchecker、公理/sorry/native_decideの3拒否、callbackを無視した恒等結果/強制失敗の2偽定理拒否を再現する。新モジュールはこのスクリプトで明示的にコンパイルし、既存lakefileのdefault targetsは変更しない。

候補範囲は `tool-patches/dynamic-effects-profile.json`、生成本体の証明範囲は `tool-patches/revm-execute-dynamic-profile.json` に固定する。生成本体3定理を全Rust/EVM対応定理3件とは数えない。元のRust higher-ranked fn pointerとLean Arrow、identity binder、trait節除去、callback実装/heap/host状態、記号評価器/抽出器/コンパイラ/LLVM/Wasm/IC/ledgerの意味論保存、実stepのGAT/命令表/PC/gasと全命令対応は未証明である。

### GAT元情報をPureへ保持する準備候補

`aeneas-gat-source-metadata.patch` はeffects候補に重ねる未採用候補である。Pureのtrait関連型は従来ID/nameのペアしか持たなかったため、型付きのtrait_associated_type_sourceとassociated_type_sourcesを追加した。元の関連型ID/nameとLLBC binder全体を保持し、寿命・outlives・型等値制約・既定型・implied trait境界を捨てない。準備処理は元情報をコピーした後、従来通りparameterized GATやimplied-bound関連型を厳格拒否する。名前の既存ペアへの射影は検証後だけ行う。完全なMemoryTr Pure宣言を抽出できたとは扱わず、全体翻訳の関連型警告も抑制しない。GAT情報がある宣言を空traitと判定しないようPureUtilsの空判定も更新する。

`GatSourceRegression.ml` は実MemoryTrの元binderとの型付き一致、寿命1/outlives1/型等値制約1/既定型/Deref境界1の保持を検査する。元GAT、parameterを空にしてimplied境界だけを残した形、implied境界を空にしてparameterだけを残した形を直接拒否する。明示的な合成ML fixtureで、元情報がない空traitは空、実GAT元情報を付けたtraitは空ではないことも検査する。このfixtureを実Rust traitの翻訳成果物とは扱わない。

`build-gat-source-candidate.sh` は分離ビルド・強制Charon MLテスト後にPure/SymbolicToPure/PureUtilsを含め基準ソースを復元する。`verify-gat-source-candidate.sh` は固定SHA、新規実step抽出、実6本体borrow-check、前候補の24否定対照と3GAT拒否、空判定fixture、全体stepのGAT厳格拒否/成果物なしを確認する。実executeも別途新規抽出し、11証跡改変拒否と新候補の生成byte一致を確認する。既に証明したexecute生成物は変わらないが、抽出器の変更そのものを形式的に証明したとは扱わない。

範囲は `tool-patches/gat-source-profile.json` に固定した。これは型付き元情報の表現と翻訳準備であり、GATのPure/Lean型族・Rust寿命の意味論・binder代入・trait implとの対応は未実装/未証明である。今回の新しい実行定理は0件。実step、全EVM、Rust heap/host、抽出器/コンパイラ/Wasm/IC/ledgerの対応も未証明のままである。

### 実MemoryTr::slice_lenの共有寿命と2経路診断

`diagnose-memory-slice-len.sh` は本番evm-core manifest/root Cargo.lockの実default `MemoryTr::slice_len` をrootに新規抽出する。実本体はreceiverを共有借用し、offset+lenをPanic付きchecked加算として評価し、Rangeのstart=offset/end=sumで実trait `MemoryTr::slice` を呼ぶ。今回のpromoted MIR構成の演算種別を監査しており、release/Wasmのoverflow設定やコンパイラの意味論保存を証明したものではない。抽出されたdefault bodyの返値はreceiverと同じFree0寿命を持つ具体的なcore::cell::Ref<[u8]>である。GAT定義/全trait実装の対応を解決したとは扱わない。

Refをopaqueにする通常経路と、実core::cell::Refのvalue/borrowフィールドを明示的に透明にする経路を比較した。透明RefはNonNullの値とFree0のBorrowRefを持つ。両経路の実default bodyは固定gat-source候補の公式borrow-checkで成功した。一方、内部NonNull/BorrowRef/std cellの全操作・borrow flag・destructorや、未知のtrait slice実装の意味論は未抽出/未証明の部分を含む。opaque依存を含む借用検査成功を、全Rust入力や物理メモリ/参照安全性の証明とは扱わない。

`audit_memory_slice_len_llbc.py` は実signatureのshared receiver/返却寿命、usize引数、checked加算、Range start/end、slice trait呼出し、返却、残存GATの寿命/制約/default/implied境界、透明RefのNonNull/BorrowRefフィールドを監査する。型・traitの意味を持つIDを保った本体shapeハッシュも固定し、各経路の加算mode/範囲反転/引数/呼出しmethod/return/GAT binderという計12証跡改変を拒否する。全操作的意味論の証明ではない。

固定SHAと新規2経路抽出、正しいborrow-onlyコマンドの成功、LeanのMemoryTr関連型による厳格拒否/成果物なしをゲートで確認した。初期の手動コマンドではborrow-checkへ非互換destを付けたためCLI拒否になったが、これは借用検査の失敗証拠ではなく、修正後の成功を採用する。範囲とstd sourceも `tool-patches/memory-slice-len-profile.json` に固定する。新しいmemory実行定理は0件。

GATを進めるには、宣言のbinder保持だけでなく、参照/代入/default normalizationとtrait impl側の関連型を正しく表現する必要がある。既存SymbolicToPureのimpl翻訳には関連型mapを無視して空typesにする経路もあり、宣言側の厳格拒否を外すだけでは対応が閉じない。receiver/返却の実寿命、Deref境界、Refのborrow token・pointer・allocation/provenance、trait slice実装、抽出器/コンパイラ/全EVM/Wasm/IC/ledger対応は未証明のままである。

### 固定std借用カウンタ判定の実生成4定理

本番evm-core manifest/root Cargo.lockから、固定nightly標準ライブラリの実core::cell::is_reading/is_writingとUNUSED定数の初期化本体を新規抽出した。wrapperや条件式に置き換えず、remove-unused-clausesも指定しない。実UNUSEDはIsizeの0、実判定はそれぞれcounter>UNUSED/counter<UNUSEDであり、heap操作や借用カウンタ更新を含まない。この3本体をgat-source候補で厳格Lean生成し、無改変byteコピーを `u256-lean/StdBorrowCounterGenerated.lean` に保存した。

`StdBorrowCounterCorrespondence.lean` の4定理は全生成Isize入力について、読取結果がval>0のBool、書込結果がval<0のBoolであり、両方trueにはならないこと、両方falseとval=0が同値であることを証明する。判定本体のResult.ok結果まで含む。これは実抽出された標準ライブラリの生成本体の定理であり、RefCell/BorrowRefが実メモリで健全であることや、Rustの全実行とLeanの比較・整数・プラットフォームモデルが対応することを証明したものではない。

`audit_std_borrow_counter_llbc.py` はScalar Isize/Bool署名、入力の転送、Gt/Ltの極性、UNUSEDのglobal参照と元initializer関数の結び付き、0リテラル、全本体操作列を監査し、コピーした証跡の8改変を拒否する。型cache定義を失う署名改変はmissing-cacheとして拒否する場合があるため、これを直接の型付き署名検査とは扱わない。実stdlib cell.rsのソースSHAも固定する。

`verify-std-borrow-counter.sh` は固定SHA・新規実抽出・生成byte一致・厳格Lean3モジュール・全生成/証明namespaceの公理監査・leanchecker・公理/sorry/native_decideの3拒否・zeroを読取trueにする/正値を書込trueにする2偽定理拒否を再現する。偽定理と同じsimp戦略で、正値の読取true・正値の書込false・zeroの両判定falseも通ることを確認し、単なるrflの定義不透明性を拒否根拠にしない。新モジュールはスクリプトで明示的にコンパイルし、既存lakefileのdefault targetsは変更しない。

範囲は `tool-patches/std-borrow-counter-profile.json` に固定する。生成本体4定理を全Rust/RefCell対応定理4件とは数えない。実BorrowRef::newのwrapping incrementとMAX/MIN境界、Cell get/set・clone/drop・borrow flag、UnsafeCell/NonNull/allocation/provenance、実memory slice実装、Rust/extractor/compiler/LLVM/Wasm/IC/ledgerと全EVMの意味論対応は未証明である。

### 実BorrowRef::newとCell内部可変性の2経路診断

固定nightlyの実 `BorrowRef::new` を、本番evm-core manifest/root Cargo.lock・promoted MIRから取得した。通常の `core::cell::BorrowRef::new` rootと `core::cell::_::new` includeを使い、wrapperや本体置換・remove-unused-clausesは使わない。impl要素を使った初期root指定は抽出器CLIで拒否され、通常パスのroot取得後にも不一致includeでは本体がOpaqueになった。これらを実本体検証の成功とは数えず、最終ゲートは透明本体の実在を検査する。

第1経路はBorrowRef/判定/UNUSEDの3本体を透明にする。実本体はCell.get、isize::wrapping_add(1)、is_readingを呼び、falseでNoneを返し、trueの継続だけでCell.replaceしてSome(BorrowRef)を返す。shared receiverと返却tokenの寿命はともにFree 0。借用検査と厳格Lean生成・生成物のLeanコンパイルは通るが、生成本体はCell型・get・replaceの3公理に依存する。生成結果は `quarantine/std-borrow-ref-new/New.lean` に無改変で隔離し、既存証明プロジェクトのtargetに加えない。公理監査は期待する3依存を確認して明示的に拒否する。型公理が存在することも、get/replaceの結果や状態更新がRustに対応することも証明されていない。

第2経路はCell/UnsafeCellの構造体とCell.get/replace・UnsafeCell.getも透明にし、計6実本体を取得する。実Cell.getのraw pointer dereferenceで借用検査とLean生成が拒否される。Lean生成ではUnsafeCell.getのraw address取得も未対応と報告される。成果物は生成されない。実Cell.replaceはshared参照からUnsafeCellのポインタを経由して可変参照を作るため、この課題を普通のimmutable値の置換で代用しない。

`audit_std_borrow_ref_new_llbc.py` は実署名/Cell Isize/受取・返却寿命、呼出し順序、wrapping増分1、判定結果の分岐極性、拒否時None/Return、更新引数、Cell/UnsafeCellの透明性を検査する。全透明本体shapeも、意味を持つ型/trait IDを維持して固定し、各経路で増分/極性/更新引数/None変種/token寿命/returnの6改変、計12コピー証跡変更を拒否する。構造監査は操作的意味論保存の証明ではない。

`diagnose-std-borrow-ref-new.sh` は固定SHA・2経路の新規抽出・12拒否・第1経路の生成byte一致/Leanコンパイル/3公理の明示拒否・第2経路のraw pointer拒否/Lean成果物なしを再現する。範囲は `tool-patches/std-borrow-ref-new-profile.json` に固定する。新規に受理した生成本体定理・全Rust/heap対応定理はともに0件。内部可変性のheap状態、Cellのaliasing/!Sync条件、UnsafeCell/raw pointerのallocation/provenance、wrapping_addの実ソースとbuiltin対応、BorrowRef clone/drop、抽出器/コンパイラ/全EVM/Wasm/IC/ledger意味論の対応は未証明である。次の対応義務は、生成成功を増やすことだけでなく、shared Cellの実状態更新を保存する意味論の実装と証明である。

### 実mem::replaceの全操作列とscalarヒープIR

前節の透明Cell経路に実 `core::mem::replace`・ptr::read/writeをincludeして再抽出した。実Cell.replaceはmem::replaceを呼び、その固定promoted MIRではread_via_copy/write_via_move intrinsicがraw pointerの読取/代入に展開されている。mem::replaceの全15文はStorageLive/Dead、shared raw address取得、旧値のCopy読取、mutable raw address取得、新値のMove書込、旧値のMove返却、Returnである。ptr::read/writeの公開wrapperはこの実依存経路で呼ばれておらず、wrapper全体の検証とも数えない。

`extract_std_mem_replace_state.py` はこの実本体を `u256-lean/StdMemReplaceGenerated.lean` の15命令へ下げる。StorageLive/Deadやlocal IDを省かず、元ABI/可変参照/返却型、generic T、RawPtrモード/pointee、unit pointer metadata、Copy/Move/WithRetag=Yesを検査する。未知の呼出し/分岐/Drop/rvalue/operandは拒否する。8種類のschema改変を拒否し、対応範囲内で読取localや書込pointerを変えた2コピーは生成IRが変わることを確認する。この2件を「schema拒否」とは数えず、operandが生成物に反映される対照とする。Python lowererの意味論保存は形式証明されていない。

`StdMemReplaceState.lean` は対象をCopyであるIsizeのscalar instanceに絞った明示的ヒープIR意味論である。Pointerはallocation/offsetから成るAddressとtagを別に持ち、heapはAddressで索引する。したがって異なるtagでも同じAddressを参照する別名は同じ更新を観測する。読取/書込権限はPointerに対する入力関数として与え、raw address取得のShared/Mutモードから権限やRustの有効性を推論しない。モデルのraw address取得はPointerを保存し、ScalarのCopy/Moveを値として処理する。これらの規則とRustのRawPtr/retag・参照/ポインタの有効性との対応は未証明である。faultはモデルの診断結果であり、Rustのpanic/UBに対応するという主張ではない。

`StdMemReplaceCorrespondence.lean` の7定理は、任意の初期化済みscalarヒープ・pointer・旧値/新値と明示的な読取/書込許可について、実操作列IRが旧値を返し、同じAddressを新値へ更新することを証明する。読取拒否・書込拒否・未初期化のモデルfaultではheapが変わらない。更新されたheapについて別名の観測と異なるAddressの不変を示し、最後の定理で実操作列の返却・別名観測・全他Addressの不変を一つに接続する。これは条件付きscalarヒープIR定理7件であり、Rust/Cell対応定理は0件である。generic非Copyの所有権/dropや任意の物理メモリ表現は対象に含めない。

`verify-std-mem-replace-state.sh` は固定SHAと本番manifest/root Cargo.lockからの新規抽出、全15命令/生成byte一致、8schema拒否・2operand反映、厳格Lean4モジュール・namespace全体の公理監査・leancheckerを再現する。公理/sorry/native_decideの3拒否、別名が旧値を保持する/他Addressも変わるという2偽観測の未解決ゴールによる拒否、同じsimp戦略による2正観測も確認する。有限対照だけでは不要simp引数のlintを無効にするが、公理監査・本体定理の厳格コンパイル・カーネル検査は維持する。既存lakefileのdefault targetsは変更しない。

結果とmem/ptr実ソースSHAは `tool-patches/std-mem-replace-state-profile.json` に固定する。現時点で残る義務は、元Rust/Charon/限定IRの意味論保存、参照とraw pointerのalignment/初期化/allocation/provenance/権限/retag、Cell/UnsafeCellのlayout・castとshared内部可変性、counterのwrap/clone/drop、非Copy所有権/drop・並行実行・コンパイラ/LLVM/Wasm/IC/ledger・全EVMへの対応である。ヒープIR証明を理由にAeneasのraw pointer拒否を解除したり、前節のCell公理を受理したりしない。

### 実UnsafeCell::getのlayout情報と未知cast効果

固定LLBCのUnsafeCell宣言にはlang item、1個のvalueフィールド、対象aarch64-apple-darwinのrepr(transparent)情報が保持されている。元cell.rsにもrepr(transparent)と、shared UnsafeCell<T>のアドレスをshared T pointerにcastし、mutable T pointerへcastする実装がある。generic layoutのchosen値は未確定で、field offsetの保証はalignment情報であり、数値offset 0や物理layoutの形式証明としては扱わない。LLBCのptr metadata表現を理由に任意のunsized/fat-pointerでcastが安全であるとも推論しない。

`extract_std_unsafe_cell_pointer.py` は同じ本番manifest/root Cargo.lock・promoted MIRの実UnsafeCell::getを識別し、全19文を `u256-lean/StdUnsafeCellGenerated.lean` へ下げる。StorageLive/Dead・local ID・shared raw address取得・pointer Copy・型付きの2 RawPtr cast・返却を保持する。元宣言のfield/transparent repr、shared receiver寿命、mutable T出力、castの元/先pointeeとconst/mutモード、unit pointer metadata、WithRetag=Yesを検査し、repr/field/ABI/cast kind/cast type/retag/returnの7コピー変更を拒否する。wrapperへの置換やunused-clause除去は行わない。限定Python lowererの意味論保存は未証明である。

`StdUnsafeCellPointer.lean` は前節のallocation/offset/tagを持つPointerとHeapを再利用するscalar-instanceポインタIRである。address取得とcastはAPIの任意の状態効果であり、Pointerのaddress/tagとHeapを変更する場合や、変更したHeapを伴うfailureも表せる。pointer CopyはIR上のidentityと定義するが、それがRustのCopy/retag/provenanceに対応することは未証明である。RawPtr/WithRetagやrepr情報を見たことだけでidentity効果に置き換えない。

`StdUnsafeCellCorrespondence.lean` の4定理のうちactual_effect_chainは、任意API/Pointer/Heapについて実19文IRの結果がaddress→cellToScalar→constToMutableの効果合成と一致することを証明する。外部効果の成功/identityは仮定しない。actual_identity_conditionalはaddress/castがPointer/Heapを保存して成功する明示IdentityContractsを前提とする。identity_modelはそのIRモデルの証人、conditional_alias_observationは同契約下で別名Address/tag/Heapを含む結果の観測を接続する。モデルと条件付きIR定理4件であり、物理Rust/UnsafeCellの対応定理は0件である。

`verify-std-unsafe-cell-pointer.sh` は固定SHAと新規実抽出、layout/全19文/生成byte一致、7schema変更拒否、厳格Lean4モジュール、namespace全体の公理監査・leancheckerを再現する。公理/sorry/native_decideを3拒否し、契約を持たないAPIのcastがallocationを変える場合と2回目のcastが失敗する場合を正対照として受理する。同じsimp手順で、それでも元pointerを返す/成功するという2偽主張を未解決ゴールとして拒否する。有限対照では不要simp引数lintだけを無効にし、本体定理の厳格検査は維持する。既存lakefile/default targetsを変更しない。

範囲は `tool-patches/std-unsafe-cell-pointer-profile.json` に固定する。実Rust/Charon/限定IRの意味論保存、物理repr/layout・alignment/初期化/provenance/権限・RawPtr/copy/retag・cast契約、unsized/fat-pointer、Cellフィールド射影とshared内部可変性、counter/clone/drop・trait memory slice、コンパイラ/LLVM/Wasm/IC/ledger・全EVM対応は未証明である。前節のmem::replaceヒープIRとの型/呼出し/状態の接続も、実Cell get/replaceを対象にさらに閉じる必要がある。Aeneasのraw pointer拒否や隔離Cell公理は受理していない。

### 実Cell::get caller/calleeのscalar状態IR接続

実Cell::getの本体は全10文であり、Cell.valueフィールドへのshared参照生成、実UnsafeCell::getの呼出し、返却raw pointerからのScalar Copy読取、Returnを含む。呼出しのon_unwindはStorageDead(1)とUnwindResumeの2文である。Cell宣言にもrepr(transparent)が保持されるが、これだけで物理field offset・shared参照生成・retag/provenanceが安全であると証明したとは扱わない。

`extract_std_cell_get_state.py` は前節の実UnsafeCell本体/layout監査を併用し、Cell/UnsafeCellフィールド型とgeneric代入、対象layout、shared receiver/出力、フィールド射影0/shared参照/unit metadata、呼出し先の型付きidentity・引数/返却型代入、raw Scalar読取、WithRetag=Yes、正確な2文unwindと全main local/storage操作を検査する。`StdCellGetGenerated.lean` には全10 main文を出力する。repr/field/callee/argument/projection/metadata/retag/unwind/returnの9コピー変更を拒否する。unwind2文は監査するが、IRのfault伝播はRustのunwind/drop/UB観測を抽象化するものであり、unwind意味論の形式対応証明ではない。

`StdCellGetState.lean` は前節のPointer/HeapとScalarを共用し、field projection・UnsafeCell::get・Scalar読取を任意の状態効果として扱う。各効果の返却Pointer/Heapやfailure Heapを後続へ渡す。linkedAPIのUnsafeCell呼出しは手書きのidentity関数ではなく、前節の実19文IRを同じPointer/Heapで実行する。field参照生成とscalar readの物理Rust意味論は依然未証明である。

`StdCellGetCorrespondence.lean` は5主定理と1効果合成helperを持つ。任意API/Pointer/Heapについてcallerのprojection→callee→readの結果が一致し、実calleeを接続するとprojection→raw address→cellToScalar→constToMutable→readの5効果に展開される。Pointer/tag/Heapの途中変化・failureを消さない。全初期化済みScalarヒープから値を返してHeapを保つ主張と、読取拒否時にHeapを保つ主張は、projection/cast identity契約と明示的read権限を前提とする。projection failureの変更済みHeapも保持する。条件付き/モデルIRの5主定理・helper1件であり、物理Rust/Cell対応定理は0件である。

`verify-std-cell-get-state.sh` は本番manifest/root Cargo.lockからの新規抽出、caller10文/callee19文両方の生成byte一致・layout/unwind/型監査、9コピー拒否を実行する。基礎Heap定義・callee4モジュール・caller4モジュールを厳格再コンパイルし、namespace公理監査とleancheckerを通す。公理/sorry/native_decideの3拒否、projectionでallocationを変えた先の値を読む/失敗時の変更済みHeapを保持する2正対照、元addressを読み続ける/失敗Heapを元へ戻す2偽観測の未解決ゴールによる拒否を、同じsimp手順で確認する。有限対照では不要simp引数lintだけを無効にする。既存lakefile/default targetsを変更しない。

範囲は `tool-patches/std-cell-get-state-profile.json` に固定する。元Rust/Charon/Python lowerer/caller/callee IRの意味論保存、物理Cell/UnsafeCell layout・field shared Ref/retag/provenance/権限・Scalar read、Rust unwind/drop/UB・unsized/non-scalar、実Cell replaceとBorrowRef更新/clone/drop・memory slice、コンパイラ/LLVM/Wasm/IC/ledger・全EVMの対応は未証明である。これらの契約を証明済みとして採用せず、隔離Cell公理とAeneasのraw pointer拒否も維持する。

### 実Cell::replaceの所有権と全unwind経路

`audit_std_cell_replace_llbc.py` は固定実Cell::replaceのmain28文とnested cleanup20文を監査する。mainにはshared Cell.value参照、実UnsafeCell::get、Mut→Mut→TwoPhaseMutのreborrow、実mem::replaceと新値のMoveがある。drop flag local9はfalse初期化の後trueになり、UnsafeCell::get呼出し時にはtrue。mem::replaceに渡す前、flagをfalseにしてから新値をlocal2からlocal8へMoveする。したがって同じcleanup形でも、2回目の呼出しでは元local2を再Dropする分岐を通らない構造である。これは抽出された操作順序の監査であり、Rust実行の所有権健全性や「二重Dropが絶対にない」という対応定理ではない。

両呼出しのon_unwindはlocal9を条件に、trueの場合だけlocal2をPrecise Dropする。元Destruct trait参照とgeneric T代入、Dropのkind/対象を検査する。Drop中に再unwindするとStorageDead(9,2,1)の後UnwindTerminate、通常cleanupは同StorageDeadの後UnwindResumeとなる。Bool定数はConst位置だけでなくSwitch branchに定義されたcacheからも解決する。source signature/callee/generic args・new-value Move・reborrow・full body shapeを固定し、意味を持つlocal/type/trait IDsを残してspan/commentとstatement/block IDsだけを正規化する。

`diagnose-std-cell-replace.sh` は本番manifest/root Cargo.lock・promoted MIRから新規抽出し、固定source/tool SHA・28 main/20 cleanup全体・役割/型/flag/destructor/terminateの監査を再現する。first flag、clear順序、Move元、TwoPhaseMut、callee、call value、Drop極性/対象/kind、double-unwind、returnの11コピー変更を拒否する。同じ抽出から既存Cell get・UnsafeCell get・mem replaceの3生成IRがbyte一致することも確認する。範囲は `tool-patches/std-cell-replace-protocol-profile.json` に固定する。

新規形式定理は0件。whole Cell::replace IRを成功経路だけで作ると、generic TのDrop効果・変更済みheap・resume/terminateや、calleeに所有権を渡した後のcleanup責任が欠落する。次の接続では、全main/cleanup、任意destructor効果、値のMoveとDrop flag、mutable/TwoPhaseMut reborrowを意味論に含める必要がある。元Rust/Charon/Python監査/IRの対応、物理shared内部可変性・layout/provenance/権限・raw read/write、非Copy所有権/drop、counter更新/clone/drop、コンパイラ/LLVM/Wasm/IC/ledger・全EVM対応は未証明のままである。
