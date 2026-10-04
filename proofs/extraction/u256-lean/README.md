# 実験段階のU256実装対応

ruint 1.20.0の実体をCharonで抽出し、Aeneasで生成した `U256Generated.lean`。
Rustの呼び出し入口は `../u256-rust/`。root Cargo.lockと同じruint版・featuresを使う。
標準div_ceil、unchecked_add、mask、array更新、加減算ループも実体を含む。
独自公理・sorry宣言はない。Leanカーネルが定義を受理した。

実験版の抽出器には `../tool-patches/` の4個のパッチを使った。
既存の `verify-rust-lean.sh` やCIは固定配布版を維持する。
UB設定は固定Rustと同じRuntimeChecks::value(session)でBoolへ定数化する。
UB加算は既存checked-addモデルへ出力し、成功証明により非overflowを要求する。
通常実行のRustでUBになる入力の挙動を証明するものではない。
全MIR付きsysrootはMiri向け設定を含むため、通常バイナリとの同値性も未証明。

`U256Correspondence.lean` の7定理は、抽出した桁演算の整数式対応、
非overflow時の事前条件検査の成功、定義済みのunchecked加算、0以上4未満のループ添字の安全な加算、
256ビットのマスク値、同幅でのmaskedの恒等性を証明する。
`U256Loops.lean` の10定理は、実抽出のarray読み書き・添字更新の安全性、
4桁ループの終了、overflowing/wrapping加減算とRust入口の全入力での成功を証明する。
これは結果値の算術仕様の証明ではなく、終了・定義済み性の証明。
`U256Arithmetic.lean` の12定理は、実抽出array更新・各桁演算・4桁ループを
整数値へ結び、wrapping加減算本体と検証用Rust入口の全入力算術対応を証明する。
R = 2^256、V = little-endian桁列の整数値として、全Word入力a,bで成功し、
加算結果は `(V(a) + V(b)) % R`、減算結果は `(V(a) + R - V(b)) % R`。
ループのcarry/borrowを含む正確な整数式も証明した。
U256部分83宣言、gasと命令結果分類も含む合同226宣言の公理監査、warning-as-error、leancheckerで成功。
**EVM命令のstack/gas/例外処理、通常Rust/Wasmバイナリ、
全MIR/Miri設定と本番ビルドの同値性、翻訳器の正しさは未証明。**

Rust compiler・Charon/Aeneas・標準演算モデル・実験パッチの正しさは信頼境界。
Charonの全Rust/OCamlテストが成功した。UIテストは478 passed、4 ignored。
このパッケージを既存の配布版による証明結果へ混ぜない。

```sh
cd proofs/extraction/u256-lean
lake build
lake env lean -DwarningAsError=true U256Correspondence.lean
lake env lean -DwarningAsError=true U256Loops.lean
lake env lean -DwarningAsError=true U256Arithmetic.lean
lake env lean -DwarningAsError=true Audit.lean
lake env leanchecker U256Correspondence
lake env leanchecker U256Loops
lake env leanchecker U256Arithmetic
```

実験バイナリがbuild-profile.jsonに記録されたSHA-256と一致する場合は、
`bash scripts/verify-u256-experimental.sh`でRustソース・features・再生成一致・補題・監査を検査できる。
このゲートは抽出されたU256加減算の全入力算術対応を検査する。
U256の全メソッド・EVM全命令・本番バイナリとの同値性は含まない。

Charonの全target Clippyも成功。make clippyの--fixはarchive sourceのVCS不足で停止したため、
自動修正なしで同じ対象を検査した。

## 固定revm gas計上の全入力対応

`RevmGasGenerated.lean` は検証用クレートではなく、本番root Cargo.lockと
`crates/evm-core/Cargo.toml` の依存解決でrevm-interpreterの実gas.rsを抽出した。
`RevmGasCorrespondence.lean` の2定理は任意Gas状態と任意u64 costについて、
`record_cost` がcost≤remainingのときtrueと差を返し、不足時はfalseで状態を保持すること、
`record_cost_unsafe` が不足時にtrueを返し、remainingを差の2^64剰余へ更新することを証明する。
両方ともlimit/refunded/memoryを保持する。名前にunsafeがある後者もRust宣言自体はsafe。
この証明は関数本体の全入力仕様であり、gas!や命令tableから停止・復元までを含まない。

Aeneasの未使用関数除去は、元々local関数だけをrootとして保持し、Charonで
明示選択した外部のstarted_from関数を落としていた。
`aeneas-selected-foreign-roots.patch` はその選択済み関数もrootとして保持する最小変更。
実Gas LLBCの前後比較で、2実関数が除去されなくなることを確認した。
既存U256の再生成と全入力証明も回帰検査に成功。抽出器パッチは信頼境界。

```sh
bash scripts/verify-revm-gas-experimental.sh
```

## 固定revm命令結果の全入力分類

`RevmResultGenerated.lean` は本番root依存profileで実instruction_result.rsの
`is_ok` / `is_revert` / `is_error` / `is_ok_or_revert` を抽出した。
`RevmResultCorrespondence.lean` の3定理は全32種類で4関数が成功し、
成功・revert・errorのちょうど1つが成立すること、okOrRevertが前2者の論理和であること、
成功がStop/Return/SelfDestructに限られ、revertが実装の6種類に限られることを証明する。
これは分類関数の全入力対応であり、命令が適切な結果を生成することや、
halt後のjournal復元、IC外部callbackの対応は含まない。

`bash scripts/verify-revm-result-experimental.sh` でソース固定、再生成一致、
警告検査、合同226宣言の公理監査と独立カーネル再検証が成功した。

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

新しいBitwiseCorrespondence名前空間の独自公理・sorry・native評価負例3件も監査が拒否した。
