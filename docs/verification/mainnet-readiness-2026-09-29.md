# Mainnet 公開前検証（2026-09-29）

## 判定

**公開承認は保留。** ローカルの実行・保持・復元テストは進んでいるが、
稼働版からの移行と release endpoint guard が未完了。
本番への install / upgrade / stop / 送金は実行していない。

対象は HEAD `88c7b73a2aba5e57707156e8082520bf5c51dd78` に、未コミットの
EVM fork 固定・legacy receipt 手数料修正・Lean/revm 検証・本稿の追加テストを
加えた作業ツリー。HEAD 単体の検証結果ではない。

## 確認済み

| 確認 | 結果 |
| --- | --- |
| `cargo check --workspace` | 成功 |
| gateway release Wasm ビルド | 成功 |
| `evm-db` / `ic-evm-core` / `ic-evm-gateway` の lib・統合テスト | 497件成功、失敗・skip なし |
| Verus（CI 指定版 0.2026.05.05.d03e906） | 197 verified、0 errors |
| 全 workspace Clippy、既存書式・API・依存分離ガード | 成功 |
| `scripts/predeploy_smoke.sh` | 12件成功、既存 timing-sensitive テスト1件 skip |
| upgrade → snapshot 復元 → 再取引 | 残高・nonce・receipt 保持と再取引成功 |
| wrap → upgrade → unwrap | トークン残高・nonce・wrap request 保持、ledger への払い出し成功 |

追加した lifecycle テストは **同じ候補 Wasm 同士** の upgrade。
過去の稼働版からの移行、進行中の外部送金の復旧、全ての状態の保存を証明するものではない。
RPC E2E は postprocess 前の Wasm を使用する。

`CI_LOCAL_MODE=github scripts/ci-local.sh` は Solidity テストまで成功した後、
`npm` 不在で停止。npm 10.9.4 を一時導入し、残りの同一コマンドを個別実行した。
ラッパーの一括 exit 0 は未取得だが、残りの RPC gateway の `npm test` / `npm run build`
と `scripts/run_canbench_guard.sh` は成功した。
依存監査（cargo-deny / cargo-audit）は既存の advisory ignore を適用して成功。
Solidity は18件、wrap E2E は追加ケースを含め6件成功。
Lean/revm の `scripts/verify-revm.sh` も再実行して成功した。
verification-policy ガードは base ref 未指定のため skip であり、PR 用判定は未実施。

性能ガードは origin/main の baseline と比較し、9項目が基準内
（instructions の差分は -5.71%～0%）。canbench 0.4.1 が使用した実ランタイムは
自動取得した PocketIC 10.0.0 ARM64。生成された計測値は
`/private/tmp/kasane-mainnet-canbench-results.yml` に保存し、既存 baseline は変更していない。
性能計測後に profiling feature なしの本番 Wasm を再ビルドし、同一 SHA-256 を確認した。

## 本番の読み取り

3月13日の既存運用記録から得た `4c52m-aiaaa-aaaam-agwwa-cai` に対して、
匿名 `dfx canister info` と `dfx canister call --query` のみを実行した。
ユーザーが言及した半年稼働環境と同一かは未確認。

- module hash: `747ea3db560e601f3d2fcef43798cb695ce333493ffb2fceb500a0a5e2dde7af`
- `mode=Normal`、`needs_migration=false`、`critical_corrupt=false`、`safe_stop_latched=false`
- `schema_version=6`。候補の `CURRENT_SCHEMA_VERSION` は **8**。
- `tip_number=158`、`queue_len=0`、`mining_error_count=0`、`prune_error_count=0`
- `decode_failure_count=1`、最後のラベル `unwrap_request`。解消済みかは未確認。

この時点の query は半年の連続稼働や負荷耐性の証拠ではない。
稼働 module hash に対応する commit / Wasm と障害処置記録を取得し、
schema 6 の実データを使って隔離環境で schema 8 への移行を検証する必要がある。

## Release artifact の未解決事項

`scripts/release_wasm_guard.sh` は postprocess 後の endpoint 照合で失敗した。
`ic-wasm 0.10.0` は以下の Candid `composite_query` を通常の query と扱う。
Wasm 側は同名の `canister_composite_query` を出力している。
公式 0.11.1 の `check-endpoints` でも同じ誤判定を再現した。

- `quote_native_withdrawal`
- `rpc_eth_call_object_at`
- `rpc_eth_call_object_with_query_precompile`

ガードの除外設定は広げていない。ツールの修正と同じ最終 artifact に対する
E2E が完了するまで、生成ファイルを検証済み release と扱わない。
0.11 系は `optimize` コマンドも廃止しているため、単なる最新版への置換では
既存 postprocess スクリプトは動かない。

今回生成したファイルの SHA-256:

| Artifact | SHA-256 |
| --- | --- |
| `ic_evm_gateway.wasm` | `f7642c074ed1c2ddc800165850e10769d5c9851b774739cabe16c8b1a8d2a831` |
| `ic_evm_gateway.release.final.wasm`（guard 不合格） | `49385919a32e3e2b9d8a182605c84e169c7066f15dce5a6a3169d6513295eb1d` |

## 復旧手順の修正

`docs/ops/chain-state-migration.md` の snapshot restore 後の reinstall を削除。
snapshot は Wasm・heap・stable memory を復元する。reinstall は復元した状態を
消してしまう。外部 ledger の送金は巻き戻らないため、再開前の照合が必要。
根拠: [ICP snapshot 仕様](https://docs.internetcomputer.org/guides/canister-management/snapshots/)。

## 再現環境

- macOS ARM64、Rust 1.97.1、Verus 用 Rust 1.95.0
- Foundry 1.8.3、PocketIC server 12.0.0 ARM64
- cargo-deny / cargo-audit は一時ディレクトリに導入
- 元の `POCKET_IC_BIN` は存在しないパスを指し、リポジトリ同梱 PocketIC は
  x86_64 だったため、公式 ARM64 版を明示して実行
- `CARGO_HOME=/Users/0xhude/.cargo` を明示して sandbox 内のキャッシュ選択失敗を解消

実行ログはこの端末の `/private/tmp/kasane-mainnet-*.log` に保存。
一時ログ・ツールのパスは永続的な CI 設定ではない。

## 追加調査（同日）

### 稼働版と移行経路

- 匿名で module hash / ops status を再取得し、hash・schema 6・decode count 1 に変化なし。
- 稼働版の公開 `candid:service` を取得し、当該ファイルの全履歴245 revision と比較。
  `d37340e3e80ace2803e667a0ec50ede032b4baba`（2026-03-16）と内容一致。
  **インターフェース一致は実装やビルドの一致を意味しない**ため、採用 commit は未確定。
- `.icp` のキャッシュ・ローカル replica と release 出力の35ファイルを SHA-256 照合。
  本番 module hash と一致する Wasm はなかった。
- GitHub release 一覧と Actions artifact 一覧はいずれも空。
  HEAD の GitHub CI は成功しているが、未コミット候補や過去デプロイの証拠にはならない。
- schema 6→7 は `e73ee0cf`（2026-06-20）、7→8 は `cf8c117f`（2026-07-10）。
  候補の migration は旧 schema 全般から現行へ進む実装だが、既存 gateway migration
  テストの開始値は `current - 1`（7）。同版 upgrade テストと合わせても、
  **稼働版の schema 6 生データを読めることは未検証**。
- 旧版は別 wrap canister を使う構成。候補の統合構成への移行では
  `scripts/lib_legacy_wrap_drain.sh` が要求する旧 request 一覧の照合も必要。

### unwrap decode failure

- 最終検出日時は **2026-03-12 09:55:21 JST**。
- `corrupt_log::record_corrupt` は検出の累積カウンタ。同一時刻の重複を抑制するため、
  件数は未処理 request 数でも、破損 request の厳密な個数でもない。
  Candid が一致した revision の同関数でも同じ挙動を確認した。
- 3月13日の smoke 記録にある request
  `3d7465cad9eea04caaf62d3679dc5a728308ca96bf4f774d40c494c868e93004`
  を本番 `get_unwrap_dispatch_overview` で照会し、`Dispatched`・`error=null` を確認。
  これは当該 request の状態であり、前日の decode failure の解消証明ではない。
- ローカル運用レポートに当該 decode failure の処置記録は見つからなかった。
  公開 counter は request ID を保持せず、公開 API に全 unwrap request の一覧もない。
  障害時ログ、request ID、または管理者が保有する状態バックアップとの照合が必要。
- 現在の公開 metrics は submitted=165、included=158、dropped=7、queue=0。
  最終 block 時刻は2026-07-08 23:59:33 JST。
  長期間 canister が存在していることと、継続した実負荷での検証は区別する。

### endpoint guard の原因確定

[ic-wasm 0.11.1 の公式ソース](https://github.com/dfinity/ic-wasm/blob/0.11.1/src/check_endpoints/candid/mod.rs#L68)
は `FuncMode::Query` と `FuncMode::CompositeQuery` の両方を
`CanisterEndpoint::Query` に変換している。
したがって今回の3件は Candid/Wasm の不整合ではなく、検証ツールの分類不具合。
原因は確定したがツールは未修正で、release guard の不合格は残っている。
`composite_query` を通常 query に変更したり、検証対象から除外したりして通すべきではない。

## 受付検査の修正と再検証（2026-09-30）

legacy/EIP-2930 の gasPrice が u64 上限を超えると、受付は成功して実行時に
INVALID_FEE で破棄される不整合を再現した。受付・置換・fee index 再構築・
実行候補の優先順・実行前検査の価格計算を実行経路と揃えた。
回帰テストは上限超の受付拒否とキュー不変、u64 上限の実行、価格による置換、
base fee 変更後の優先順を legacy/EIP-2930 の両形式で確認する。

`cargo check --workspace`、`ic-evm-core` の全 lib/統合テスト、同 crate の
Clippy（warnings 拒否）、変更 Rust の書式、`verify-revm.sh` が成功。
最新ソースから release Wasm を再ビルドし、上記の upgrade/snapshot 復元と
wrap→upgrade→unwrap の追加 E2E 2件も成功した。
9月29日の artifact hash と全 CI 結果は当時の記録であり、この追加修正の
artifact を指さない。今回、全 CI ラッパーと release artifact guard は再実行していない。
公開承認保留と稼働版移行・release guard の未完了事項は継続する。
