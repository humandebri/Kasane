# ローカル検証結果 — 2026-09-29

対象・対応元の固定値は `model-sources.sha256` / `revm-profile.json`、
意味上の範囲は `revm-correspondence.md` を参照。

| コマンド / 検査 | 結果 |
| --- | --- |
| `cargo check --workspace` | 成功 |
| `bash scripts/verify-evm-proofs.sh` の Verus `--no-cheating` | 197 verified、0 errors。受理時の一般価格式と legacy の `Some(cap)` を含む |
| `bash scripts/verify-revm.sh` | 成功 |
| Lean 4.30.0 ビルド | 明示的な定理 34 個。生成補題を含む 131 定理宣言を公理監査 |
| 公理監査 | 標準論理公理のみ。`sorryAx`・独自公理・native evaluation 公理なし |
| 純粋 Rust 関数と Lean | 431 ケース一致 |
| 実際の revm journal と Lean | 9 traces / 45 時点で storage・残高・log 内容/順序・depth 一致。別に短い action 列 486 traces を実装で確認 |
| Ethereum Prague fixtures | 38 ケースで state root / logs hash 一致 |
| Kasane CALL 経路 | 12 ケース成功。送信者 nonce、gas 徴収、receipt、受取残高、storage、logs を確認 |
| Kasane CREATE/SELFDESTRUCT | 親 commit/revert の 4 ケース成功。作成先の残存、残高移動、code hash、nonce、fee を確認 |
| untouched account の直接 commit | stable DB と state epoch は不変。touched account は書き込まれる |
| 過去に保存された空 account | stable DB にゼロ値 account が残っていても、全量再計算・差分 commit とも state root は空状態と同じ。nonce が非ゼロなら root は変わる |
| ICP update intent precompile | reverted subcall と成功した再試行の receipt log、request map を確認 |
| `cargo test --locked -p ic-evm-core --lib revm_exec::tests` | 10 テスト成功 |
| `cargo test --locked -p verified-core` | 32 テスト成功 |
| `cargo test --locked -p ic-evm-core --tests` | 34 test targets、236 件成功、0 ignored |
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
過去の実行で既に保存された空 account をこの変更だけで削除することはない。
この回帰テストは state root への影響だけを確認する。既存 canister の空 account 件数は
公開 query から列挙できず、実環境の棚卸し・削除を実施した証拠ではない。
既存依存 `proc-macro-error2` の将来互換性 warning は今回のテストでも出力された。
