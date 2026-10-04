# ローカル検証結果 — 2026-10-03

Git `32160d7ec8c3e90f5730ef36ec4107b724229d2c` と検査時点の作業差分が対象。
再現時の対応元は `model-sources.sha256` の 9 ファイルで固定する。
並行した query/update 変更が hash に含まれるが、その全挙動を Lean で証明した結果ではない。

| コマンド / 検査 | 結果 |
| --- | --- |
| `cargo check --workspace` | 成功 |
| `cargo clippy --locked -p ic-evm-core --lib --tests -- -D warnings` | 成功 |
| `bash scripts/verify-revm.sh` | 成功 |
| Lean 4.30.0 ビルド | 明示的な定理 97 個（従来比 +61） |
| 公理監査 | 生成補題を含む 255 定理宣言を監査。標準論理公理のみ。`sorryAx`・独自公理・native evaluation 公理なし |
| 公理監査の拒否確認 | `.lake/` 内の一時的な負例で `sorry`・独自公理・`decide +native` に依存する証明がすべて拒否されることを確認 |
| `lake env leanchecker KasaneEvm` | 成功。保存済み証明を Lean kernel で再検査 |
| 純粋 Rust 関数と Lean | 439 ケース一致 |
| Rust サイズ検証関数と Lean | 146 ケース一致。output/log 数/topics/data の上下境界と、2 個目の log だけが不正なケース |
| 両資産 precompile の入口と Lean | 256 ケース一致。4 CALL scheme × target/bytecode 一致 × Transfer/Apparent × static × 外部許可、2 precompile。全ケースで account/log の副作用なし、許可時だけ空 ABI の parse エラーへ進む |
| journal / Prague / Kasane 回帰 | 9 journal traces、486 short implementation traces、38 Prague fixtures、CALL/CREATE/SELFDESTRUCT と precompile rollback/authorization の既存回帰が成功 |
| サイズ超過後の共有 cache と再試行 | RETURN/REVERT 出力超過、log data 超過、log 数超過を検査。sender nonce/balance、contract storage/balance、recipient balance、stable state epoch と手数料受取 account の不変、後続送金の永続化を確認 |
| `bash scripts/verify-verus.sh` | 未検証。`verus` 実行バイナリが見つからず終了。下記 2026-09-29 の結果を今回の再検証結果には含めない |

CI に `evm-proofs` job を追加し、同じ `verify-revm.sh` を実行する設定にした。
GitHub Actions 上の実行、全 CI、Wasm/PocketIC、本番 deploy は今回の検証に含めない。
本番 Rust の挙動を変更せず、証明・比較テスト・回帰テスト・検証ゲートを更新した。
既存依存 `proc-macro-error2` の将来互換性 warning が出力された。

`Refinement.lean` は **Lean の journal と論理 storage map の対応**を全入力で証明する。
Rust からの翻訳、実際の diff の完全性、stable memory/codec、opcode 全体、非同期 ledger/IC runtime、
CREATE/SELFDESTRUCT のモデル意味論は依然として未証明。

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
