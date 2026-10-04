# 外部挙動の証明境界

`ledger-profile.json` は同梱ledger Wasmのリリース、公式Gitコミット、
レビューしたRustファイルのSHA-256を固定する。Gitタグからコミットを確認した。
公式ビルドイメージ・Rust 1.93.1・Bazel 8.6.0で再ビルドし、同梱Wasmと全バイト一致を確認した。
詳細は `ledger-reproduction-profile.json`。これはartifact対応の確認であり、
Rustコンパイラの意味論やIC実行基盤の形式証明ではない。

`../evm/KasaneEvm/Ledger.lean` は ledger core の `apply_transaction` と
`purge_old_transactions` を参照した手書きモデル。
古すぎる時刻、未来時刻、重複、適用の順序と、block timestamp + window + drift
より後で履歴を削除する条件を扱う。
成功時の `created_at_time ≤ block_time + drift` とpurge条件から、
履歴削除後の同一転送はtoo oldになる。保持中はduplicate等となり適用されない。
単調に進む任意の時刻列で任意回数再送しても、成功後の適用回数は1。

この定理には固定hashと固定created_at_time、成功時刻の条件、単調時刻を要求する。
タイムスタンプなしの転送、変更した引数での再送、費用・残高・allowance、
mint/burn、upgradeによる仕様変更、Durationからu64への変換trap、hash衝突、
並行処理・キュー・callbackの実装対応・送達や成功の保証を含まない。
IC replicaや外部ledgerの全挙動を証明したと解釈しない。

Kasane側のquote固定、fee変更時の再計算制約、Duplicateの成功扱いを
`crates/ic-evm-gateway/src/lib.rs` で確認した。この呼び出し側全体の抽出対応は未証明。
VerusとRust抽出で証明したdispatch判定関数から、callback実装全体への接続が必要。

## revm全命令への接続で確認した具体的な義務

固定revmのADD実装をCharonで抽出し、AeneasでLean変換を試した。
Rust抽出ではStackTr/MemoryTrのGATについて3警告。
Lean変換は `interpreter_types.rs:210` のmutable referenceを返す `top` の処理で
`Can't end abstraction 3 as it is set as non-endable` と失敗した。
未使用の `StackTr::top` を除外するとMemoryTrのGATで停止する。
MemoryTrも除外するとInterpreterTypesからそのtraitへの参照が残り、正常なLean生成にならない。
これらの試行はvendor内の単独Cargo.lockを使っており、本番rootの依存解決とは別。
本番ではruint 1.20.0を固定しているため、試行を本番profileの対応証拠として扱わない。
実スタックの `popn` / `popn_top` にはunsafe実装がある。
この呼び出しを公理に置き換えて全命令同値と主張することはできない。

再現する場合はrepo rootで以下を実行する（生成物は一時領域）。

```sh
export RUSTUP_TOOLCHAIN=nightly-2026-09-17
export DYLD_LIBRARY_PATH="$PWD/.local/proof-tools/aeneas/libs" # macOS
.local/proof-tools/aeneas/charon cargo --preset=aeneas --sysroot default \
  --start-from 'revm_interpreter::instructions::arithmetic::add' \
  --dest-file /private/tmp/kasane-revm-add.llbc \
  -- --manifest-path vendor/revm/crates/interpreter/Cargo.toml
.local/proof-tools/aeneas/aeneas -backend lean -dest /private/tmp/kasane-revm-add \
  -namespace RevmAdd -use-lean-modules false -abort-on-error -warnings-as-errors \
  /private/tmp/kasane-revm-add.llbc
```

全命令の義務はstackのメモリ安全性と意味論、U256演算、stack under/overflow、
static/dynamic gas、memory拡張、fork gate、host/db、CALL/CREATE/SELFDESTRUCT、
crypto precompile、例外時のjournal復元、および命令tableの完全性。
現在のPrague fixtureとtrace比較は回帰検査であり、これらの全入力同値証明ではない。

ruint 1.20.0の桁上がり・桁借り関数については、本番と同じfeaturesで実体を抽出し、
全入力の整数式対応を `../extraction/lean/WordCorrespondence.lean` で証明した。
これをU256の4桁ループとEVM命令実装へ接続する義務は残る。

U256加減算の本体とループはCharonで抽出できた。`apply_mask`、`from_limbs`、
`mask`、`ZERO`、`MASK`、`SHOULD_MASK`、`LIMBS`、`nlimbs`と
`core::num::_::unchecked_add` を限定includeすると、Aeneasは標準unchecked_addの
`ub_checks`とunchecked演算で停止した。`--monomorphize --mir optimized`
と `--rustc-arg=-Copt-level=3 --rustc-arg=-Zmir-enable-passes=+Inline` の試行では、
標準array Defaultの単相化に対しAeneas PrePasses.ml:167が内部エラーとなる。

抽出済み桁演算を任意桁数で合成するLeanモデルの整数式・剰余式と、
4桁の基数が2^256であることは `../extraction/lean/LimbComposition.lean` で証明した。
本番U256のarray操作とこのモデルの接続は未証明。

全MIR付きsysrootと専用Cargo targetで再抽出すると、`ub_checks`とAddCheckedの停止を越え、
標準unchecked_addのprecondition_checkで`Unexpected result: Cps.Unit`となった。
固定Charonのpanic正規化はPanic/PanicFmt/BeginPanicのlang itemと2個の名前を扱うが、
`core::panicking::panic_nounwind_fmt`は含まない。同関数は固定Rustソースで`!`を返し、
PanicInfoのcan_unwind=falseを通じてabortする。抽出LLBCでは同関数がopaque callとなり、
戻り先のない関数末尾へUnitが到達する。

`../extraction/tool-patches/charon-panic-nounwind-fmt.patch`はこの関数を既存panic正規化へ
追加する実験パッチ。実験版のビルドとU256再抽出に成功したが、既存の証明ゲートでは使用しない。
版と取得したビルドソースのハッシュは同ディレクトリのbuild-profile.jsonに記録する。
OCaml 5.3.0・pkgconf・GMPは検証専用の無視対象領域に導入し、OCaml依存もローカル環境へ導入済み。
既存配布版のCharon/Aeneasと証明済みの結果は変更しない。

修正版Charonではprecondition_checkがmassertへ変換され、U256加減算のLean定義を
最後まで生成できた。ただし標準check_language_ubとusize::div_ceilが未知定義として残る。
この2個を追加includeした試行の結果も記録し、独自公理を残した生成物は証明済み扱いしない。
Charon実験版の全Rust/OCamlテストは後続の検証で成功した。既存ゲートの固定配布版は維持する。

追加のCharonパッチでUB RuntimeChecksをRust codegenと同じvalue(session) APIにより定数化し、
AeneasパッチでUB加算を既存checked加算モデルへ出力した。未知定義を残さないU256生成が
Leanに受理され、再抽出一致・7補題・37宣言の公理監査・leancheckerが成功。
UB検査が有効でも非overflowならprecondition_checkが成功することを別に証明した。
全体対応は未証明であり、`../extraction/u256-lean/README.md`に現在の範囲を記録する。

Charonのmake testが成功した（UI 478 passed・4 ignored、unit 20、Cargo回帰11など）。
archive sourceのためmake clippyの--fixはVCS不足で停止し、同じ全targetを
自動修正なしのcargo clippyで追加検査する。実験ゲートは本番と同じruint版・featuresと
4個の実装ファイルのSHA-256、3パッチ・バイナリのSHA-256を固定する。

U256実抽出への算術接続が進んだ。`U256Arithmetic.lean` は実array更新、
桁上がり/桁借り、4桁ループ、mask、wrapping加減算、検証用Rust入口を通して、
全256ビット入力で和・差の2^256剰余と結果値が一致することを証明する。
配布版と分けた実験ゲートが再抽出・監査・kernel再検証を行う。
翻訳器とパッチ・全MIR/Miri設定と本番Rust/Wasmの対応は信頼境界に残る。
EVM命令のstack・gas・例外処理との接続、U256の他の演算、外部IC/ledger全体は未証明。

本番root Cargo.lockでADDを再抽出しても、MemoryTrのGATがAeneas変換を止める。
この試行では`StackTr::top`だけを除外した。traitを公理へ置き換えた成功扱いはしない。
一方、同じroot依存profileの実Gas::record_cost/record_cost_unsafeは抽出できた。
明示選択した外部関数を保持するAeneas prepassの最小パッチを追加し、2関数について
全入力の不足判定・remaining更新・limit/refund/memory保持の整数仕様を証明した。
独自公理を追加せず、再生成一致・警告検査・95宣言の合同公理監査を検査する。
これはgas計上本体の対応であり、opcode table、gas!、halt、journal復元との接続は未証明。

ADD本体を明示includeして同じroot依存profileで変換すると、trait変換より先に
`instructions/macros.rs:134` の `popn_top().unwrap_unchecked()` で
`Can't copy a mutable borrow` が出た。先のroot試行は外部関数本体をincludeしていなかった。
具体EthInterpreterで実ADD/SUBを呼ぶ単相化プローブではCharonが成功するが、
Aeneasのarray Default prepassで内部エラーが出る。revm版・std featureとruint版は
本番と合わせたが、依存グラフ全体の一致は証明していない。
`../extraction/diagnostics/revm-add-extraction.json` に実ソースハッシュと停止位置を記録した。
失敗生成物を証明済みとしてゲートへ取り込まない。

実instruction_result.rsの分類4関数は、本番root依存profileから抽出・再生成できた。
全32種類で成功/revert/errorが完全かつ排他的で、成功とrevertの正確なconstructor集合も
Leanで証明した。合同226宣言の公理監査・カーネル再検証に成功。
この証明は分類本体の対応であり、実命令からhaltやjournal復元への接続は未証明。

固定ledgerのTimeStamp::add/subも確認した。Duration.as_nanos().try_into().unwrap()
でu64へ変換後、saturating_add/subする。したがってTimeStampの加算自体は飽和する。
`LedgerSaturation.lean` は全u64時刻に対し、飽和算術による古さ・未来・保持条件が
既存Natモデルと一致することを証明し、任意回数再送の二重適用防止を引き継ぐ。
checked Duration narrowingが成功する条件、固定hash/created_at_time、単調時刻が必要。
これは公式ソースを参照したモデル間の対応であり、実Rust/Wasmとの対応は未証明。

Kasane refresh_transfer_quote_after_rejectionは通常のCharon cargoではnative cdylibの
ICシンボルリンクで止まった。cargo checkのRustc依存引数をCharon rustcへ渡し、
同じdefault sysrootを使うとリンクなしで実関数の抽出に成功した。
Aeneasはcore/src/str/pattern.rs:99（SymbolicToPureTypes.ml:392）の未実装で停止する。
文字列prefix判定を公理へ置き換えず、成功したRust抽出と失敗したLean変換を区別する。

固定ledger TimeStampの構築・読み取り3メソッドは、そのままの公式Rustソースから
抽出し、9定理で全入力の値対応・往復・checked生成の成功/失敗条件を証明した。
再抽出一致と合同254宣言の監査を検査する。詳細は
`../extraction/ledger-time-profile.json` と `../extraction/u256-lean/README.md`。
checked overflow profileの証明であり、公式release/Wasmとの同値性は未証明。
Add/Subをstd Duration内部まで完全に抽出すると、範囲制約型TPattern未対応で停止する。

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

隔離した未採用Charon/Aeneas候補では、エラー型のない実ADD/SUB fixtureの
4本体（ラッパー2本体を含む）が記号的な借用検査を通過した。65個のOpaque宣言の
内部は未検査で、unsafe stackや外部呼出先の実装対応の証明ではない。
同じ候補でLean変換はMemoryTrのGATで停止する。再現診断は
`bash scripts/verify-revm-add-sub-borrow-candidate.sh`、範囲とhashは
`tool-patches/revm-add-sub-borrow-profile.json`を参照する。

固定revmの全命令テーブル（既知150opcode、未知106byte）を照合した診断用
151ラッパーと85種類の実命令本体は、候補Charonで毎回エラー型なしに抽出できる。
`diagnose-revm-instructions.sh`は236本体の名前を照合するが、273個のOpaque宣言の
内部と別probe graphの本番対応は未証明。全出力の候補借用検査は
MemoryTr::slice_lenの具体的RefとGAT戻り型の不一致で停止する。
この診断は全EVM命令や外部host挙動の形式証明ではない。

宣言RPITIT戻り型を保持する未採用Charon候補では、trait宣言の戻り型を
既定実装の具体型へ固定せず、GAT射影として保持する。最小例で借用スライス/
override Vecの区別を検査し、全命令の236本体も再抽出できた。
借用検査は従来の戻り型不一致を越えてDeref/GAT射影比較で停止する。
再現は`verify-rpit-signature-candidate.sh`、scope/hashは
`tool-patches/rpit-signature-profile.json`。候補変換の形式的正当性と、
全EVM命令およびIC/ledger外部挙動の証明は未完了。

GAT/Derefの停止点ではregion viewの生存期間だけが異なり、消去型は一致する。
固定Aeneas型解析では実際の既定スライス戻り型はborrow=true、対応するGAT宣言は
falseとなる。`verify-gat-borrow-obligations.sh`はこの実解析の差を再現し、
安全な交差/包含に必要な借用位置モデル11定理も監査・カーネル再検査する。
型と借用位置の対応やOCaml比較器の意味保存は未証明で、全EVM/IC/ledger証明へ
算入しない。比較器の等値チェックを単純に緩める修正は行っていない。

TimeStamp Add/Subの本体は、そのままの固定公式ソースからDurationの宣言を
opaqueで保持する分割抽出に成功した。DurationとDebug/unwrapを明示的な外部モデルとして
与えた場合について、全モデル入力でのu64変換・成功/panicの必要十分条件・飽和加減算と、
u64ナノ秒ラッパーの全域性を15モデル定理で証明した。実Rust境界テスト2件、
再抽出一致、59宣言の厳格な公理監査・警告/カーネル検査・不正公理等の負例3件も通過。
これは外部モデルを含む条件付き結果で、Duration/Debug/unwrapやエラーaliasの実Rust
refinement、panic/unwinding、IC/ledger非同期処理・実Wasmまでの対応は未証明。
完全Duration抽出は範囲型TPatternで停止することを引き続き負例として検査する。
再現は`../../scripts/verify-ledger-duration-conditional.sh`、
境界の詳細は`../extraction/ledger-duration-profile.json`を参照。

Durationを丸ごとモデル化した経路をさらに進め、実std Duration型・from_nanos/as_nanos・
定数2個まで変更なしで抽出した。NanosecondsはU32と<10^9の制約を持つ明示的な
外部型・生成/読取りの境界モデルとして残す。この条件下で実Durationの全入力に対する
各フィールド・往復・unchecked生成の範囲条件・u128算術の非overflowを含む19定理を証明した。
旧モデルとの往復と2演算の一致も5定理で証明し、86宣言の厳格な公理監査、独立カーネル
検査、実Rust境界テスト3件、再抽出一致と負例に成功した。
NanosecondsのTPattern/transmuteの実Rust意味論、Debug/unwrap観測、panic/unwinding、
native/Wasm・全EVM・IC/ledger非同期外部挙動までのrefinementは未証明。
再現は`../../scripts/verify-ledger-duration-source.sh`、固定範囲と義務は
`../extraction/ledger-duration-source-profile.json`。

Nanosecondsの実new_unchecked/as_innerの型付きtransmute本体もエラーなしで
抽出できた。ただしopaque型経路では固定Aeneasが両方向のunary castを拒否する。
完全な範囲型経路の最初の停止を進める未採用のU32定数範囲分類候補を隔離検証した。
型・範囲を消さずに借用/outlive解析を通過するが、次のPure型変換がTPatternを拒否する。
範囲/署名/実本体の保持、20組の分類比較、12件の不正/未対応範囲の拒否を確認した。
固定版と候補の失敗を毎回再現し、出力を証明済みとして取り込まない。
再現は`../../scripts/verify-u32-range-analysis-candidate.sh`。
これは未対応の進展と候補の範囲確認で、実Rust unsafe/全EVM/IC・ledger対応の
証明ではない。既存の19条件付きDuration定理・5対応定理のゲートは再検証した。

未採用のPure範囲型候補では、U32定数範囲の両端をconst genericに保持し、Leanの
`{ rangeValue : Std.U32 // lower ≤ rangeValue.val ∧ rangeValue.val ≤ upper }`
へ出力する。実Nanosecondsの完全型（0..999999999）と実Duration.as_secsを
変更なしで再生成できた。既存の制約付きモデルとの往復・値保持・範囲保持と
実getterの全入力結果を5定理で証明し、11宣言の公理監査、独立カーネル再検証、
公理/sorry/native_decideの負例3件に成功した。
実unsafe new_unchecked/as_innerの両方向のtransmuteは引き続き厳格に拒否される。
候補翻訳器の意味保存、Rustのpattern型validity・レイアウト・unsafe変換、
native/Wasm・全EVM・IC/ledger外部挙動の対応は未証明。候補は通常ゲートへ未採用。
再現は`../../scripts/verify-u32-range-pure-candidate.sh`。実ソースに由来する型の
制約がLeanまで残ることを確認した範囲であり、unsafe本体の対応証明は含まない。

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
再現は`../../scripts/verify-nanoseconds-read-candidate.sh`。旧条件付きDuration経路の
読取りモデルを実本体抽出へ進めた範囲であり、候補の意味保存証明を代替しない。

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
再現は`../../scripts/verify-nanoseconds-construct-candidate.sh`。実ソース由来の
算術と妥当性前提をLeanまでつなげた範囲であり、翻訳器の意味保存は未証明。

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
再現は`../../scripts/verify-ledger-duration-full.sh`。Nanoの手書き境界を減らしたが、
IC/ledgerの外部挙動に必要な非同期・実Wasm・環境の対応義務は残る。

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
再現は`../../scripts/verify-ledger-duration-typed-error.sh`。エラー型の手書き境界を
実型抽出へ進めた範囲で、実IC/ledger全体対応の証明ではない。

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
再現は`../../scripts/verify-ledger-actual-unwrap.sh`。固定範囲は`ledger-actual-unwrap-profile.json`と
`tool-patches/never-call-profile.json`（extraction配下）。候補は通常ゲートへ未採用。

### Panic argument boundary

`LedgerPanicPayload`の14本体抽出は、実`unwrap_failed`からのpanic_fmt引数を保持する。
40条件付き定理・153宣言の厳格公理監査・独立kernelと負例ゲートを検査済み。
通常presetで引数が消える対照も再現した。外部fmt constructors、Str/error formatting、
panic runtimeは明示モデルであり、実メッセージ整形、unwind/trapやICの応答同値性は未証明。
詳細と固定profileは`../extraction/README.md`のActual unwrap_failed節を参照。
再現はリポrootで`bash scripts/verify-ledger-panic-payload.sh`。

Actual formattingコンストラクタ3本も含む17本体の抽出は成功したが、
実ArgumentTypeのFnPtr fieldをAeneasが翻訳できずLean出力前に停止する。
再現: リポrootで`bash scripts/diagnose-ledger-formatting-source.sh`。
これを外部formattingの証明として数えない。ABI/receiver pointer/indirect callの意味保存が次の義務。

閉じたFnPtrの署名を保持するPure marker候補は実ArgumentTypeの型宣言段階を通過した。
ただしlate-bound regionを持つfn itemと配列参照→NonNull transmuteで実本体の翻訳が停止する。
再現: `bash scripts/diagnose-ledger-closed-fnptr.sh`。13閉signatureのmetadata保持は有限Native検査であり、
callable意味論・unsafe ABI・外部formattingの証明ではない。Lean出力/新しい対応定理はまだない。

Function region候補ではlate-bound metadataを残し、実コンストラクタのrty停止を越えた。
次の厳格停止はtrait method→FnPtr castとref→NonNull transmute。Lean出力はまだ0。
整数depthの6モデル補題をカーネル検査したが、ML/Rust/ABI/外部応答の意味保存は未証明。
再現は`bash scripts/diagnose-ledger-fn-region.sh`、詳細はextraction READMEのFunction binder節。

同じ隔離候補でtraitメソッドの関数項目署名を解決し、実new_debug/new_displayの
2箇所で変換先FnPtrとの完全一致をNative検査した。欠落trait/methodと高階trait binderは拒否する。
これは署名解決の有限検査で、ポインタ値、間接呼出し、外部formattingの証明ではない。

Exact item reification候補では実2 castの署名一致を条件にsymbolic演算を保持し、
関数値の局所region inventory/値型消去/ETY検査を揃えた。14 Native変異と実castの3変異を拒否し、
17本体診断が通った。次はsignatureの参照を値のborrowとして扱うFnPtr greedy expansionで停止する。
ref→NonNull transmute、Pure callable型、関数identity/間接呼出し/ABI/外部formattingは未証明。
再現: `bash scripts/diagnose-ledger-fn-reify.sh`。詳細はextraction READMEのExact function-item節。
Lean出力/新しいRust対応定理は0であり、実行時の外部挙動を証明したとは扱わない。

Function-pointer value借用候補は署名の参照をstored borrowとして数えず、外側/隣接/入れ子の
借用flagsを保持した。実29 FnPtr、58 context組、Rust/Miri各3観測と3compile-fail対照を検証した。
17本体診断は以前のFnPtr greedy expansion停止を越え、実safe→unsafe FnPtr transmuteで停止する。
この変換のreceiver pointer/ABI/間接呼出しと外部formattingは依然未証明で、Lean出力/新対応定理は0。
再現: `bash scripts/diagnose-ledger-fn-value.sh`、詳細はextraction READMEのFunction-pointer value節。

FnPtr representation候補は実safe→unsafe pointer transmuteをsymbolic演算として保持した。
22 Native対照と実3 target変異を拒否し、33 function region predicateのlocal/free区別を確認した。
private receiver/callback pair probeの5観測はRust/Miriで通り、wrong receiverのMiri専用対照は
allocation bounds違反で拒否された。このprobeは固定coreそのものの抽出証明ではない。
次はhigher-ranked TFnDefのPure型表現で停止する。unsafe call ABI/receiver/存続期間、
open FnPtr callable表現、reference→NonNull、CFI/IC/Wasm/外部runtimeは未証明。
再現: `bash scripts/diagnose-ledger-fn-transmute.sh`。Lean出力/新Rust対応定理は0。

Trait function-item候補は実4型のSelf/trait証拠を通常のPure generic引数に置き、
source metadata/binderを保ったまま型代入を可能にした。4再構成、20付替え、60拒否対照を確認。
実17本体では型変換の停止を越えたが、function castの値変換で停止する。
Lean出力/新Rust定理は0。callable/receiver/ABI/IC/ledger外部挙動の証明は未完了。
再現: `bash scripts/diagnose-ledger-fn-item.sh`。詳細はextraction READMEのTrait function-item節。

Pure function-item coercion候補では両端の型を演算に保持し、既存の実署名照合を再利用する。
両端への型代入と方向の区別はsynthetic operatorの回帰テストで確認した。
実ソースでは自由Self型を含むキャスト先FnPtrのPure表現が未対応で、演算生成前に停止する。
実cast値のPure変換/意味論、外部runtimeとの対応は未証明。Lean出力/新Rust定理は0。
再現: `bash scripts/diagnose-ledger-fn-cast.sh`。

Open FnPtr template候補では自由型をPure型引数に捕捉し、ABI・局所寿命・引数構造を保持した。
実16 signatureの完全再構成/代入一致、64型変数ID付替え、192拒否対照を確認した。
実17本体はitem-to-pointer型変換/operand照合を越え、次のPure transmuteで停止する。
有限回帰検証であり、全入力変換定理・callable/外部実行の対応証明は未完了。
Lean出力/新Rust定理は0。再現: `bash scripts/diagnose-ledger-fn-template.sh`。

Typed Pure FnPtr transmute候補は両端の型と演算を保持し、既存representation guardとoperand照合を使う。
実2演算の型保持/自由capture代入と、22変形/3他backendの型変換前拒否を確認した。
実17本体の次の停止はtrait function-itemの値変換。call ABI/receiver/provenance/lifetimeや
IC/ledger外部実行の証明は未完了。Lean出力/新Rust対応定理は0。
再現: `bash scripts/diagnose-ledger-fn-pure-transmute.sh`。

Trait function-item値候補は元型の局所寿命と値側の消去済み寿命を区別し、identityと
Self/trait証拠を保持する。実2定数は既存PrePassesで処理して検証し、12不正値/18型対照等を拒否した。
実17本体は16/17まで進み、残る停止はArguments.newの参照→NonNull transmute。
有限回帰検証であり、Rust関数値意味論・call ABI/receiver・IC/ledger外部実行の証明は未完了。
Lean出力/新Rust対応定理は0。再現: `bash scripts/diagnose-ledger-fn-item-value.sh`。

NonNullの補助source抽出は、基準入力のOpaque/空layoutを、追加includeだけで実transparent
NotNull raw-pointerフィールド/SizeOf・AlignOf記号保証まで展開した。全検証設定と17本体を維持した。
array参照→NonNullの5有限観測はRust/Miriで通り、ゼロ長配列のelement dereferenceは
Miri専用負例で拒否された。Native/Pure/Lean変換やallocation/provenance/lifetimeの全入力証明ではない。
以前の16/17 Aeneas候補の停止は維持する。Lean出力/新Rust定理は0。
再現: `bash scripts/diagnose-ledger-nonnull-layout.sh`。

NotNull pointer型候補は追加NonNull実定義を厳密なprofileで受け入れ、制約/mutability/参照先型を保持する。
格納borrow分類とpointee region比較を対応し、外側/隣接borrow flags・自由寿命追跡を維持した。
実定義込みでも16/17まで進み、残る停止はArguments.newの参照→NonNull変換。
実pointer生成/provenance/lifetime/外部実行の証明は未完了。Lean出力/新Rust対応定理は0。
再現: `bash scripts/diagnose-ledger-notnull-pattern.sh`。

配列参照→NonNullの実ソース表現ガードは追加した
(`../extraction/tool-patches/array-nonnull-profile.json`)。
既存body-region消去後の実2箇所の型/透明layout照合と46負対照、Rust/Miri各5観測は通る。
Arguments.newは共有borrowからのpointer region/provenance対応を明示して16/17で停止する。
この停止を解消するポインタ値意味論・元借用寿命の対応定理は未追加。
Lean出力0で、Rust全入力同値性・EVM全命令・IC/ledger外部実行の未証明状態は変わらない。

配列参照→NonNullは次候補
(`../extraction/tool-patches/array-nonnull-regions-profile.json`)で実共有borrowのIDからloanを検索し、
型消去との一致を検査して完全なpointee region metadataをSymbolic出力へ保持した。
実Arguments.newのSymbolic評価後、Pure pointer value extraction義務で16/17に停止する。
Native終了region保持/13不適合loan対照、Rust/Miri各5観測は通る。
実allocation/address/provenance意味論、Rust元lifetime対応、Leanとの全入力対応は未証明。
この型保持の進展をIC/ledger外部実行の証明として数えない。

最新array-nonnull-pure候補は実Arguments.newを含む17本体のPure変換と後処理を通り、
実本体中の2つの共有array→NonNull演算にborrow/shared/loan Symbolic IDが残ることをNativeで直接確認した。
21不正origin/loan/destination対照、3他backend、Rust/Miri各7観測が通る。
これらの識別子をRust実allocation/addressとはみなさず、同じarray再借用のaliasも観測している。
Lean抽出は未証明のNonNull pointer typeで停止し、不完全Types.leanを隔離する。
完成したLean成果物/新Rust対応定理は0。Rust全入力・EVM全命令・IC/ledger外部実行の未証明状態は変わらない。

pointer値対応の準備として`SharedArrayPointerFootprint`の14 MODEL定理をLeanカーネル検証した。
任意array長/byte sizeの必要範囲条件、空extent・失効temporal権限の正サイズread拒否と
address-only/symbolic-content-ID-only同一視の反例を形式化した。
これは実Rust pointer/refinement定理ではなく、初期化・型validity・aliasing・concurrencyを含む
read安全性は未証明。実ctx→Rust allocation/provenanceのaliasを尊重するadapterが必要である。
既存Aeneas pointer抽出は拒否を維持し、Rust全入力/EVM全命令/IC・ledger外部実行の未証明境界は変わらない。

## 固定公式ledgerの再現ビルド

固定コミット`cf41372e3d4dc1accfe2c09a7969f8bddc729dc1`を浅く取得し、
既存レビュー済み4 Rustファイルのhash一致とクリーンなGit treeを確認した。
公式rust-toolchain/Dockerfile/filesから算出したic-buildタグのイメージを用い、
linux/amd64環境のBazel `local`/`stamped`設定で
`//rs/ledger_suite/icrc1/ledger:ledger_canister.wasm.gz`をビルドした。
公式publishはこの対象をcopyするため、不要な全canister bundleを作成していない。
1575 action/736.183秒で正常終了し、解凍Wasmの全バイトが同梱ファイルと一致した。
SHA-256は`a273d741019b4324b22e2e64cfb0239de148ae9693e8801102a04118dfd383c0`。
埋め込みコミットIDとCandid serviceも固定ソースに一致する。

`scripts/audit_icrc_ledger_artifact.py`はcommit/レビュー済みソースhash/同梱Wasm hash/
埋め込みmetadata/Candid bytesと、指定した再ビルドWasmの全バイトを比較する。
命令の意味論やWasm全体のvalidationを行うパーサではない。
7誤値・破損・重複metadata対照と、metadataを保った変更artifactの拒否を確認した。

再現: `LEDGER_REPRO_CONTAINER_NAME=<新しいタスク用名> bash scripts/reproduce_icrc_ledger_wasm.sh`。
既存同名containerがあれば停止し、再起動・削除は行わない。
終了した`kasane-ledger-repro-cf41372`と公式イメージ、専用source/cacheを今後の検証用に残している。
ホストhome・SSH鍵・他リポはマウントしていない。
ソースは`.local/proof-tools/ic-ledger-source`、成果物は
`.local/proof-tools/ledger-repro-cache/ic-icrc1-ledger.rebuilt.wasm`。

実ビルドはRust1.93.1/wasm32であり、既存Charon抽出のnightly2026-09-17/
aarch64-apple-darwinとは別。Rust1.93.1のNonNull fieldは通常のconst raw pointerで、
scalar-valid-range compiler属性が非null制約を持つ。nightly抽出はNotNull pattern型を持つ。
標準ライブラリ・target/layoutの対応義務は残る。
既存証明のscopeをこのWasmへ自動的に拡張しない。source→Wasmのコンパイラ意味論、
Rust/Lean全入力、IC外部実行の形式的対応は依然未証明である。

## 公式Rust1.93.1・wasm32のMIR版対応

`release_duration_probe.rs`の4つの標準ライブラリ呼び出しを、公式固定イメージ内の
Rust1.93.1・wasm32でMIR出力した。これはledger本体の抽出ではなく、
`-Copt-level=z -Zmir-opt-level=3`を指定したstandalone probeである。
同じprobeの既存nightly/aarch64出力と比較すると、`duration_nanos`関数のMIR全本体は一致する。
変換上限u64::MAX、nanos計算の係数1e9、saturating演算の構造も確認した。
`audit_release_duration_mir.py`で4つの算術/上限/比較/演算改変を拒否する。
このテキスト照合はMIR意味論の証明ではなく、一般MIR parser/verified compiler passでもない。

成功側の式が共通でも、checked変換errorはreleaseで`TryFromIntError(())`、
nightlyで`TryFromIntError(PosOverflow)`となる。unwrapのinline/制御フローも異なり、
全MIRは同一ではない。既存のnightlyのTypedError/panic証明をreleaseの
エラーpayload・Debug表示・trapへそのまま接続しない。
実timestamp wrapper・MIR意味論・scalar transmute/layout・失敗時挙動と
compiler/target対応の義務は残る。固定記録は`ledger-release-mir-profile.json`。
専用MIRコンテナ`kasane-ledger-release-mir-cf41372`は停止済みで検証用に残している。

### 公式ledger_coreの実コンパイルから得たTimeStamp MIR

`ledger-core-release-mir-profile.json` は、固定公式ledgerのBazel依存グラフから得た唯一の `ledger_core` Rustcアクションを記録する。コンパイラ自身は Rust 1.93.1、ターゲットは wasm32-unknown-unknown。公式ソース・全依存・edition2024・最適化/LTO/target-feature設定・環境変数を保持し、変更した引数は `--emit` と `--out-dir` の2箇所のみ。元のrlib/dep-info出力を作業キャッシュ内のMIR出力に置き換えた。ソースファイルの切り出し・derive削除・代替依存は行っていない。

`ledger-core-release-timestamp.mir` は得られた全crate MIRからそのまま保存した実add/subの2本。`as_nanos -> TryInto<u64> -> Result::unwrap -> saturating_add/sub -> TimeStamp` の各呼び出しと戻り先を含む。前節の単体プローブに追加した `-Zmir-opt-level=3` は今回は付けておらず、標準ライブラリの各処理は外部呼び出しとして残る。従って単体プローブのインライン済み算術やエラーペイロードを、そのままこのMIRの本文と同一視できない。

取得手順は公式ビルドと同じ `--config=local --config=stamped` で、`aquery --output=jsonproto 'mnemonic("Rustc", filter("ledger_core", deps(//rs/ledger_suite/icrc1/ledger:ledger_canister.wasm.gz)))'`。そのJSONと `bazel info execution_root` のディレクトリを使い、`scripts/replay_ledger_core_mir.py` が元のprocess_wrapperを再実行する。各証跡は `.local/proof-tools/ledger-repro-cache` に保持した。`python3 scripts/audit_ledger_core_release_mir.py` は固定SHA、出力のみ2箇所の変更、全MIRと保存本文の一致、呼び出し経路を確認し、コンパイル引数4件・MIR経路4件の改変を拒否した。

これは実ソースと実ビルド設定のMIR証跡であり、MIRの操作的意味論や標準ライブラリ呼び出しのLean対応、LLVM/Wasmコンパイラ、panic/trap、IC実行の証明ではない。既存の条件付き定理の仮定は解消していない。専用 `kasane-ledger-core-mir-cf41372` は終了・停止済みで、証跡とキャッシュを保持する。

### 実TimeStamp MIR由来の限定呼び出し列の条件付き定理

`release-timestamp-sequence-profile.json` は `ledger-core-release-timestamp.mir` の実ローカル番号から生成した2本の呼び出し列と、11件の補題・条件付きLean定理を固定する。生成スクリプトは元コンパイル引数・全MIR・保存本文の固定SHAと監査を通した上で、`_4`へtimestamp読出し、`_7`へ時間値、`_6`へ変換結果、`_5`へunwrap結果、`_3`へ飽和演算結果という実番号を取り出す。生成結果は `ReleaseTimestampGenerated.lean`。フロントエンド自体の形式的正しさは未証明で、汎用MIR変換器とは扱わない。

`ReleaseTimestampSequence.lean` の限定IRは番号付き有限ローカルストアを解釈し、数値・変換結果・成功値・panic・不正ローカルを区別する。全てのモデル有効DurationとU64 timestampについて、外部呼び出し契約 `CallContracts` が満たされる場合、生成した加減算列は既存時間値モデルの `min(u64::MAX,t+d)` / `t-d` と一致し、時間値がu64上限を超える場合はpanic観測へ進む。制御フローを任意の時間値関数について証明してから、既存のDurationモデルへ適用している。

契約はas_nanosの全域算術、TryIntoの上限判定、unwrapの成功値またはpanic、飽和演算の全域算術を仮定する。変換エラーのペイロードはUnitで抽象化し、panicメッセージ・trap/実行終了・復帰は扱わない。StorageLive/Dead、参照の生成、move、unwind unreachable、レイアウト・provenance・有効Rust値との対応も限定IRへの抽象化義務として未証明。Rust1.93.1標準ライブラリの実契約、LLVM/Wasm、ICと外部ledger全挙動の証明はこれによって閉じていない。

`bash scripts/verify-release-timestamp-sequence.sh` は成功。4つの新LeanファイルをwarningAsErrorで検査し、11件の指定定理と全namespaceの公理依存を監査し、`leanchecker`でも確認した。公理・sorry・native_decideの注入3件、不正ローカルから成功する誤定理1件を拒否した。既存の実MIR監査の8件の改変拒否も通過する。今回の証明をRust↔Leanの無条件全入力同値性として数えない。

### 基本ブロックから呼び出し列への意味論保存

`release-timestamp-cfg-profile.json` は公式実MIRの5基本ブロックから生成した加減算CFGと、11件の新しい限定IR定理を記録する。各ブロックのローカル番号と実 `return: bbN` を読み取り、`ReleaseTimestampCFGGenerated.lean` に保存した。`lower_correct` は任意の有限fuel・ブロックグラフ・初期ストア・全域の型付き呼び出しインターフェースについて、CFGから列への変換が成功した場合、限定CFG意味論と既存呼び出し列意味論が同じ値/panic/不正ローカル観測を返すことを示す。加減算それぞれの生成CFGをfuel5で変換すると、前回の実ローカル番号付きプログラムに定義的に一致する。従って前回の外部呼び出し契約を仮定した全モデル入力対応を、このCFGにも適用できる。

この証明は有限fuelの判定を無条件の実行終了とは扱わない。fuel4で今回の5ブロックが変換できないことも証明した。変換先の列の実行では、unwrap失敗は後続の飽和演算や戻り先へ進まずpanic観測を返す。

`extract_release_timestamp_cfg.py` は元の固定MIR/引数を監査し、未知の文・終端後の文・重複ブロック・範囲外戻り先・unwind形式の変更という5改変を拒否した。範囲内の戻り先変更はCFGへ反映され、勝手に元の一本道へ正規化しない。`verify-release-timestamp-cfg.sh` の4新ファイルのwarningAsError、全namespace/11定理の公理監査、独立leanchecker、6否定対照は通過した。否定対照には変換ブロックを飛ばしたCFGや不足fuelを元プログラムとして受理する誤定理の拒否を含む。

未証明の境界は残る。本文から限定ブロックASTへのPython抽出の形式的正しさ、StorageLive/Dead・参照生成・move・unwind unreachableの抽象化、Rustレイアウト/provenance/型有効性、実Rust1.93.1標準ライブラリ契約、LLVM/Wasm/IC外部挙動は閉じていない。今回形式的に閉じたのは、生成された限定ブロックASTから呼び出し列への意味論保存である。以前の11件の条件付き列定理は変更していない。

### 実MIRの全ローカル寿命イベント

`release-timestamp-lifetime-profile.json` は、実TimeStamp加減算の成功経路それぞれからStorageLive/Dead、copy、borrow、move、全callのmove引数、returnを省略せず取得した20イベントと、16件のローカル寿命モデル定理を記録する。`ReleaseTimestampLifetimeGenerated.lean` の列は加減算で同一となる。実MIRでStorageLive/Deadのないローカル0/1/2をフレーム中常時liveとし、引数1/2のみ初期化済み、戻り先0は未初期化とする。

一次情報は固定 [Rust1.93.1のmir/syntax.rs](https://github.com/rust-lang/rust/blob/1.93.1/compiler/rustc_middle/src/mir/syntax.rs)。同版はStorageLiveの再実行による未初期化へのreset、StorageDeadの重複をnopとすること、MIR Moveの具体的な作用が未確定であることを明記する。モデルもliveをreset、deadを割当/初期化の削除として扱う。Moveに関しては未初期化にする/保持する両方の方針を用い、成功経路が両方でチェックを通ることを証明した。実Rust Moveの完全な意味論を選択・確定したとは扱わない。

チェックでは各copy/borrow/move/call/returnの読出しにliveかつ初期化済み、書込み先にliveを要求する。フレームの全数値に依存しない列の全20イベントと任意長の全接頭辞について、検査が成功する。呼び出しのpanicが実際にどの接頭辞で止まるか、呼出し失敗時に書込まれる値とcommitの対応は別の未証明義務であり、この成功経路スケジュールから失敗時の戻り先初期化を主張しない。参照2→8はローカル初期化のイベントとして扱い、alias/provenance/retagや借用期間の証明ではない。

`verify-release-timestamp-lifetime.sh` は4新LeanファイルのwarningAsError、16指定定理と全namespaceの公理依存監査、独立leancheckerを通過した。公理/sorry/native_decideの注入3件と、元20イベントからStorageLive8を削除する・callの引数7を未初期化9に変える・return0を既にdeadの4に変えるという誤列の成功主張3件を拒否した。これはローカル寿命/初期化モデルの機械検証であり、Rustの全メモリ安全性やMIR↔モデルの無条件全入力同値性ではない。Python抽出の形式的正しさ、実行トレースの対応、実型・参照・標準ライブラリ、LLVM/Wasm/IC全挙動は未証明のまま。

### ローカル寿命チェッカーの健全性

`release-timestamp-lifetime-soundness-profile.json` は6件の追加MODEL定理を固定する。`step_sound` は任意の状態・イベント・Move方針について、checkerがstepを受理した場合、そのイベントが宣言する各読出しローカルはliveかつ初期化済み、各値書込みローカルはliveであることを証明する。StorageLive/Deadは割当状態の遷移であり、値を読む/書くイベントとしては列挙しない。`check_sound` はこれを任意のイベント列へ持ち上げ、実ソース由来の加減算20イベントと全接頭辞についてSafeTraceを証明する。読み書きの値・型有効性・実メモリ位置をこの述語へ追加したわけではない。

`verify-release-timestamp-lifetime-soundness.sh` の固定SHA、2新LeanファイルのwarningAsError、6指定定理/全namespaceの公理監査、独立leancheckerは成功。公理/sorry/native_decide注入3件、戻り値0の未初期化読出しを安全とする誤主張、未割当9へのcall結果書込みを安全とする誤主張の計5件を拒否した。ここで閉じたのはローカルchecker成功から宣言された読書き前提への健全性であり、Rust MIR/実行トレースの抽出対応、参照/alias/provenance/retag/layout/heap、実stdlib、LLVM/Wasm/IC全挙動は未証明のまま。
