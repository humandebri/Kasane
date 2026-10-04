# Bridge の復旧と検証

wrap は処理世代を stable memory に保存する。再投入後の古い応答は資産引き落とし結果・mint・申請状態を更新しない。mint の nonce と取引IDを確定する区間に外部呼び出しを置かない。

`get_request` の `recovery_action` は次の操作を示す。値がない申請に、画面側の推測で復旧操作を追加しない。

| 値 | 操作 | 条件 |
| --- | --- | --- |
| `RetryWrap` | `retry_wrap_request` | 手数料徴収済み、引き落とし未確認、mint未投入の失敗wrap。申請者本人のみ |
| `RetryNativeDeposit` | `retry_native_deposit` | 引き落とし済みNative Depositのcredit失敗 |
| `RefundWrap` | `recover_failed_wrap` | 引き落とし済みで、mint未投入または失敗・drop確定を確認できるwrap |

wrap再試行には同じrequest IDを使い、Bridge手数料を再徴収しない。`retry_asset` に保存された資産・金額・申請者を使って、資産量とledger転送手数料のallowanceを確認する。memoと`created_at_time`を維持し、ledgerの`Duplicate`を成功として扱う。

`TooOld`、未確定mint、投入状況が不明な旧申請には、新規転送や返金を自動実行しない。ledgerの取引とEVMのreceiptを照合する。取消・Bridge手数料返金は提供しない。

## ローカル検証

最初に `cargo check --workspace` を実行する。続いてgateway・stable保存・画面のテスト、型チェック、binding整合性を検証する。

PocketICのwrap E2Eは公式ledgerを使う。`scripts/prepare_ci_icrc1_ledger_wasm.sh` で `ICP_LEDGER_WASM` を設定し、次を実行する。

```sh
cargo build -p ic-evm-gateway --target wasm32-unknown-unknown --release --locked
cargo build -p ic-evm-gateway --example bridge_delay_ledger --target wasm32-unknown-unknown --release --locked
cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --test wrap_unwrap_flow_e2e --locked -- --test-threads=1
```

`bridge_delay_ledger` は検証専用canisterである。引き落とし応答・metadata応答・metadata失敗を遅延させ、10分経過後の再投入と古い応答の到着順を再現する。

旧Wasmからの検証には、修正前に保存したartifactを指定する。

```sh
BRIDGE_LEGACY_GATEWAY_WASM=/absolute/path/to/pre-fix.wasm \
  cargo test --manifest-path crates/evm-rpc-e2e/Cargo.toml --test wrap_unwrap_flow_e2e --locked \
  bridge_upgrade_legacy_failed_wrap -- --ignored --test-threads=1
```

最終候補で `CI_LOCAL_MODE=github scripts/ci-local.sh` と `scripts/predeploy_smoke.sh` を実行する。gatewayを先にアップグレードし、schema移行の完了を待ってから再試行する。対応するfrontendはgatewayの後に適用する。既存の失敗・未確定申請を照合してから本番適用する。
