# 最終 Wasm・旧 schema 更新・外部 ledger の追加検証（2026-09-30）

## 判定

main `bcbc03948eafce8cdeffbadeb1a5d2b92c7dee7c` に対する追加検証。
最終 Wasm の endpoint 検査と E2E、履歴 schema 6 版から schema 8 への更新は成功。
ただし、履歴 artifact と稼働 module の一致、旧 wrap の全 request 照合は未完了。
この結果だけでは既存 canister の公開更新を承認しない。

本番の install・upgrade・stop・snapshot 作成・送金は行っていない。
公開 query・metadata と、controller 向けの既存 snapshot 一覧・logs を読み取った。

## 最終 release artifact

`scripts/release_wasm_guard.sh` が生成した最終 Wasm の SHA-256:

`faaaf4bfdf644e47cad6975751a408856d6b8a081ca1ba99b7f1054ac09a9cc7`

- endpoint guard: 成功。metadata 注入後の最終 Wasm を検査。
- 同じ最終 Wasm の RPC E2E: 12 件成功、既存 timing-sensitive テスト 1 件スキップ。
- 同じ最終 Wasm の wrap/unwrap E2E: 6 件成功。更新後の token 保持・ledger 払い出しを含む。
- checker 回帰テスト: 2 件成功。update/query/composite query の分類に加え、
  export の mode 不一致・欠落・追加を拒否することを確認。

checker 回帰テストは GitHub-equivalent CI にも追加した。

`ic-wasm` 0.11.1 の公式 Candid parser が composite query を通常 query に分類する
不具合を、`CanisterEndpoint::CompositeQuery` に修正した。
crate archive の SHA-256 を固定し、記録したパッチと Cargo.lock で checker をビルドする。
0.10 系の shrink/optimize と、修正した 0.11.1 の endpoint 検査を別工程にした。
endpoint の除外追加や canister API の変更はない。
根拠: [公式 parser ソース](https://github.com/dfinity/ic-wasm/blob/0.11.1/src/check_endpoints/candid/mod.rs)。

```sh
bash scripts/test_ic_wasm_endpoint_checker.sh
bash scripts/release_wasm_guard.sh
export EVM_GATEWAY_WASM="$PWD/target/wasm32-unknown-unknown/release/ic_evm_gateway.release.final.wasm"
source scripts/prepare_ci_icrc1_ledger_wasm.sh
CARGO_TARGET_DIR=target/e2e-lifecycle cargo test --locked \
  --manifest-path crates/evm-rpc-e2e/Cargo.toml \
  --test rpc_compat_e2e --test wrap_unwrap_flow_e2e -- --test-threads=1
```

ホストに合う `POCKET_IC_BIN` と Foundry の contract build artifact が必要。
`EVM_GATEWAY_WASM` は対象 2 suite の全 install/upgrade に使われる。

## 履歴 schema 6 → 8

稼働 Candid と一致した履歴 commit `d37340e3e80ace2803e667a0ec50ede032b4baba` を
Git archive から復元してビルド。元の Cargo.lock に欠けた `ic-evm-core` の `candid`
依存辺 1 行のみを補い、`--locked` でビルドした。依存バージョン変更はない。
旧 Wasm の SHA-256:

`e6aabe7ae7e54b0f7e4f9ad1a590069c3f7151d528d461b83266b5f6188d07c3`

旧 Wasm で送金データを作り、最終 release Wasm に upgrade した。
schema 6 → 8、needs_migration=false、残高・nonce・receipt の保持と再取引を確認。
旧 snapshot 復元後には schema 6 と元の状態が戻り、再取引も成功した。
`bash scripts/run_schema6_upgrade_e2e.sh` で再現できる。

この旧 artifact は稼働 module hash と一致しない。Candid の一致は実装の一致を
意味しないため、実稼働 artifact の互換性には一般化しない。
履歴 Candid を注入して postprocess した旧 Wasm も hash は
`feccebc6b7ca7309014839a65c8d5656321963fa25168de9427cc702902b8bc2` で、稼働版と不一致。
取引データは PocketIC で作ったもので、実稼働 stable memory は取得していない。
また全ての保存領域や処理中の外部送金を検査したものではない。

## 稼働環境と外部 ledger

- gateway `4c52m-aiaaa-aaaam-agwwa-cai`: module hash
  `747ea3db560e601f3d2fcef43798cb695ce333493ffb2fceb500a0a5e2dde7af`。
  schema 6、Normal、needs_migration=false、critical_corrupt=false、safe_stop_latched=false。
  tip=158、queue=0、mining/prune error=0。既存 snapshot は 0 件、取得した logs は空。
- decode_failure_count=1、label=unwrap_request。最終検出時刻は前回と同じ。
  処置記録や破損 request ID は特定できていない。
- 旧 wrap `lpuz5-uyaaa-aaaam-ah4da-cai`: module hash
  `d574bc2f40a48c48063df002bc87624bf291afbca0031a4842de5be3e8fa7cc4`。
  fee policy の ledger は `xafvr-biaaa-aaaai-aql5q-cai`。
  公開 Candid metadata がないため履歴版の Candid で既知 request を照会。
- 2026-03-13 の記録の wrap request `d2a089176ee23d399343bcf48447ae35b542433885e3e0788a03f6c9baeddaff`
  と unwrap request `3d7465cad9eea04caaf62d3679dc5a728308ca96bf4f774d40c494c868e93004` は、
  旧 wrap の get_request がともに null。gateway の後者の dispatch overview は
  Dispatched / error=null。

記録の払い出し ledger block 0x0ce902（846082）を archive
`ml4bs-lyaaa-aaaai-atgdq-cai` の get_blocks で取得した。
transfer amount=1,000,000 e8s、fee=10,000 e8s。
送金元 account `fe6456b1c2fa3f1ea75e7f4fdfee6cdeef589103cdee0a5a23d5d65ac14b2327` は旧 wrap、
送金先 account `2ffcc36ea796e31d5772327b1b5e8d39d3231e3f1b2c63875eef2bd378cb15d6` は
運用記録の受取 principal に一致した。
当該 1 件の払い出しとの一致であり、全 request の完了や過去の decode failure の解消を
証明しない。archive block query の結果は独立した暗号学的検証をしていない。

## 残る必要資料

1. 稼働 module hash に一致する旧 Wasm と、その版の状態データ。
2. 旧 wrap の全 request 一覧と各 ledger 処理の証跡。
3. unwrap decode failure の該当 request ID または障害処置記録。

現在の公開 API と空の logs からは完全には復元できなかった。
実データの snapshot を新しく作る場合は稼働 canister の停止が必要になるため、
この検証作業では実行していない。
snapshot 作成の停止要件は [公式手順](https://docs.internetcomputer.org/guides/canister-management/snapshots/) を参照。
