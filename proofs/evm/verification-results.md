# ローカル検証結果 — 2026-09-29

対象・対応元の固定値は `model-sources.sha256` / `revm-profile.json`、
意味上の範囲は `revm-correspondence.md` を参照。

| コマンド / 検査 | 結果 |
| --- | --- |
| `cargo check --workspace` | 成功 |
| `bash scripts/verify-revm.sh` | 成功 |
| Lean 4.30.0 ビルド | 明示的な定理 32 個。生成補題を含む 115 定理宣言を公理監査 |
| 公理監査 | 標準論理公理のみ。`sorryAx`・独自公理・native evaluation 公理なし |
| 純粋 Rust 関数と Lean | 431 ケース一致 |
| 実際の revm journal と Lean | 9 traces / 45 時点で storage・残高・log 内容/順序・depth 一致 |
| Ethereum Prague fixtures | 38 ケースで state root / logs hash 一致 |
| Kasane CALL 経路 | 12 ケース成功。送信者 nonce、gas 徴収、receipt、受取残高、storage、logs を確認 |
| `cargo test --locked -p ic-evm-core --lib --test phase1_fee_rules --test phase1_fee_ordering --test internal_trace_execution` | 70 テスト成功 |
| 新規 3 integration test targets の `cargo clippy -- ... -D warnings` | 成功 |
| 変更 Rust の `rustfmt --check`、shell 構文、`git diff --check` | 成功 |

fixture runner のため `serde_json` を dev-dependency に追加した。既存 lock 内の版を使用し、
本番依存には追加していない。

全 CI / predeploy smoke / Wasm 実行 / mainnet deploy は未実施。
Rust と Lean の全入力での同値性、opcode 全体の証明はこの結果に含めない。
既存依存 `proc-macro-error2` の将来互換性 warning は今回のテストでも出力された。
