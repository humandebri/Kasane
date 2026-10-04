# ローカル検証結果 — 2026-10-03

Git `32160d7ec8c3e90f5730ef36ec4107b724229d2c` と検査時点の作業差分が対象。
再現時の対応元は `model-sources.sha256` の10ファイルで固定する。
分離した `test/external-proof-correspondence` worktreeが対象。原作業ツリーの並行query/update変更を含まない。

| コマンド / 検査 | 結果 |
| --- | --- |
| `cargo check --workspace` | 成功 |
| `cargo clippy --locked -p ic-evm-core --lib --tests -- -D warnings` | 成功 |
| `bash scripts/verify-revm.sh` | 成功 |
| Lean 4.31.0 ビルド | 明示的な定理 105 個（従来比 +69）。ledgerモデル8定理を含む |
| 公理監査 | 生成補題を含む287定理宣言を監査。標準論理公理のみ。`sorryAx`・独自公理・native evaluation 公理なし |
| 公理監査の拒否確認 | `.lake/` 内の一時的な負例で `sorry`・独自公理・`decide +native` に依存する証明がすべて拒否されることを確認 |
| `lake env leanchecker KasaneEvm` | 成功。保存済み証明を Lean kernel で再検査 |
| 純粋 Rust 関数と Lean | 439 ケース一致 |
| Rust サイズ検証関数と Lean | 146 ケース一致。output/log 数/topics/data の上下境界と、2 個目の log だけが不正なケース |
| 両資産 precompile の入口と Lean | 256 ケース一致。4 CALL scheme × target/bytecode 一致 × Transfer/Apparent × static × 外部許可、2 precompile。全ケースで account/log の副作用なし、許可時だけ空 ABI の parse エラーへ進む |
| journal / Prague / Kasane 回帰 | 9 journal traces、486 short implementation traces、38 Prague fixtures、CALL/CREATE/SELFDESTRUCT と precompile rollback/authorization の既存回帰が成功 |
| サイズ超過後の共有 cache と再試行 | RETURN/REVERT 出力超過、log data 超過、log 数超過を検査。sender nonce/balance、contract storage/balance、recipient balance、stable state epoch と手数料受取 account の不変、後続送金の永続化を確認 |
| `bash scripts/verify-verus.sh` | 固定版 `0.2026.05.05.d03e906` で198 verified、0 errors。arm64 macOS公式配布のSHA-256一致を確認して導入 |

CIに `evm-proofs` と `rust-lean-correspondence` jobを追加し、
`verify-revm.sh` と `verify-rust-lean.sh` を実行する設定にした。
GitHub Actions 上の実行、全 CI、Wasm/PocketIC、本番 deploy は今回の検証に含めない。
本番 Rust の挙動を変更せず、証明・比較テスト・回帰テスト・検証ゲートを更新した。
既存依存 `proc-macro-error2` の将来互換性 warning が出力された。

`Refinement.lean` は **Lean の journal と論理 storage map の対応**を全入力で証明する。
Rustからの翻訳は別パッケージ `../extraction/` で13関数を抽出して全入力対応証明を追加。
実際のdiffの完全性、stable memory/codec、opcode全体、非同期ledger/IC runtime、
CREATE/SELFDESTRUCT のモデル意味論は依然として未証明。

## 追加した実装対応・外部モデルの範囲

`bash scripts/verify-rust-lean.sh`: 本番Rustの再抽出一致、全入力対応13関数とruint桁演算2関数、
90定理宣言の公理監査、Lean 4.31.0と`leanchecker Correspondence`・`leanchecker WordCorrespondence`で成功。
標準演算と翻訳器の正しさは信頼境界。詳細は `../extraction/README.md`。

ledgerの固定コミットに沿ったモデルでは、固定hash・固定created_at_time・単調時刻を条件に、
成功後の任意回数の再送で適用回数が1のままであることを証明した。
外部ledger Rust/Wasmとの全入力同値性を証明した結果ではない。

元の作業ツリーでも導入済みVerusを実行し204 verified、0 errors。
198との差は原作業ツリーの並行変更に由来するため、別結果として扱う。

## 過去の検証結果 — 2026-09-29

対象・対応元の固定値は `model-sources.sha256` / `revm-profile.json`、
意味上の範囲は `revm-correspondence.md` を参照。

| コマンド / 検査 | 結果 |
| --- | --- |
| `cargo check --workspace` | 成功 |
| `bash scripts/verify-evm-proofs.sh` の Verus `--no-cheating` | 198 verified、0 errors。価格式・legacy の `Some(cap)`・空 account 判定を含む |
| `bash scripts/verify-revm.sh` | 成功 |
| Lean 4.30.0 ビルド | 明示的な定理 36 個。生成補題を含む 142 定理宣言を公理監査 |
| 公理監査 | 標準論理公理のみ。`sorryAx`・独自公理・native evaluation 公理なし |
| 純粋 Rust 関数と Lean | 439 ケース一致 |
| 実際の revm journal と Lean | 9 traces / 45 時点で storage・残高・log 内容/順序・depth 一致。別に短い action 列 486 traces を実装で確認 |
| Ethereum Prague fixtures | 38 ケースで state root / logs hash 一致 |
| Kasane CALL 経路 | 12 ケース成功。送信者 nonce、gas 徴収、receipt、受取残高、storage、logs を確認 |
| Kasane CREATE/SELFDESTRUCT | CREATE は親 commit/revert × 旧空 account 有無の 4 ケース、SELFDESTRUCT は親 commit/revert の 2 ケース成功。code hash、残高、nonce、fee を確認 |
| untouched account の直接 commit | stable DB と state epoch は不変。touched account は書き込まれる |
| 過去に保存された空 account | stable DB にゼロ値 account が残っていても、全量再計算・差分 commit とも state root は空状態と同じ。nonce が非ゼロなら root は変わる |
| 旧空 account の revm 読み取り | `basic` / `basic_ref` は空 record を不存在として返す。runtime code を返す CREATE がそのアドレスで成功する |
| ICP update intent precompile | reverted subcall と成功した再試行の receipt log、request map を確認 |
| `cargo test --locked -p ic-evm-core --lib revm_exec::tests` | 10 テスト成功 |
| `cargo test --locked -p verified-core` | unit 33 テスト成功。統合テストも成功 |
| `cargo test --locked -p ic-evm-core --tests` | 34 test targets、238 件成功、0 ignored |
| `cargo build -p ic-evm-gateway --target wasm32-unknown-unknown --release` | 成功 |
| `bash scripts/predeploy_smoke.sh` | 成功。PocketIC RPC 互換テスト 11 成功・1 ignored |
| 対象 4 integration test targets の `cargo clippy -- ... -D warnings` | 成功 |
| 変更 Rust の `rustfmt --check`、shell 構文、`git diff --check` | 成功 |

fixture runner のため `serde_json` を dev-dependency に追加した。既存 lock 内の版を使用し、
本番依存には追加していない。
PocketIC E2E は別 Cargo.lock を使うため、証明ゲートの Cargo 出力は
`target/evm-proof` に隔離して再実行し、成功した。

今回の追加差分の全 CI / mainnet deploy は未実施。PocketIC の Wasm 実行は predeploy smoke に含む。
Rust と Lean の全入力での同値性、opcode 全体の証明はこの結果に含めない。
CREATE の親 REVERT テストで、untouched account が stable DB に残る不具合を再現して修正した。
過去の実行で既に保存された空 account をこの変更だけで物理削除することはない。
回帰テストは state root と CREATE の実行結果を確認する。既存 canister の空 account 件数は
公開 query から列挙できず、実環境の棚卸し・削除を実施した証拠ではない。
既存依存 `proc-macro-error2` の将来互換性 warning は今回のテストでも出力された。

## 価格とruint桁演算の追加検証

価格のchecked narrowingを同じ意味の明示的分岐に変更し、Verusの`None`条件を必要十分にした。
`cargo check --workspace`、価格のunit test 5件、既存feeプロパティテスト、
`cargo clippy -p verified-core --all-targets --locked -- -D warnings` が成功。
変更した `fee.rs` のrustfmt検査と `git diff --check` も成功。
workspace全体の `cargo fmt --all -- --check` は既存の他のverified-coreファイルの
整形差分で失敗した。それらのファイルは今回変更していない。

`verify-revm.sh` を価格変更後に再実行し、Lean監査・比較・回帰検査が成功。
`verify-rust-lean.sh` は本番13関数とruint桁演算2関数の再生成一致、90宣言の公理監査、
両対応証明のwarning-as-error検査・カーネル再検査に成功した。
追加したWordCorrespondenceの名前空間でも、sorry・独自公理・native評価公理の負例を
公理監査がすべて拒否することを確認した。
Aeneasバックエンドの既存namespace warning 3件はビルド時に再表示される。

任意桁数の桁列モデルについて、抽出済みruint桁演算の合成が桁数を保ち、
正確な加減算式と剰余式を満たすことを証明した。4桁の基数は2^256。
`LimbComposition`も独自公理・sorry・native評価公理を拒否する監査とカーネル再検査に含む。
本番U256のarray/update・ループ・maskへの実装対応はまだ含まない。

### 実験版U256加減算の全入力算術対応

`proofs/extraction/u256-lean/U256Arithmetic.lean` は実抽出した本番と同版・同featuresの
ruintについて、配列更新と桁演算を合成し、wrapping加減算と検証用Rust入口の
全入力での結果値が和・差の2^256剰余に一致することを証明した。
配布版による13本番関数+2桁関数の証明とは別の実験パッチによる結果。
翻訳器・標準演算モデル・全MIR/Miri設定と本番ビルドの対応を信頼する条件付きであり、
EVM全命令や本番Wasmの実装対応を証明したとは扱わない。

### ledgerの飽和時刻算術

固定コミットのTimeStamp::addはDuration.as_nanos()をu64へchecked narrowingし、
saturating_addする。`LedgerSaturation.lean` の9定理はこの算術のモデルについて、
created/nowが全u64範囲にあるときの年齢・未来・履歴保持判定が既存Natモデルに一致し、
固定hash/created_at_timeと単調時刻で任意回数再送しても成功後の適用回数が1であることを証明する。
Duration narrowingが失敗する入力、実ledger Rust/Wasm、hash計算、残高/allowance、
callbackや送達の対応は含まない。source profileに公式timestamp.rsの固定SHA-256を追加した。

全モデルで114明示定理、生成補題を含む310宣言の公理監査、Leanカーネル再検証、
439既存Rust比較ケースが成功。新規モジュールのwarning-as-errorとleancheckerも成功。
