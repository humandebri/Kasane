# ChainState 72->88 Migration Runbook

## 目的
- `ChainState` wireのサイズ変更（72->88）を、後方互換コードなしで安全に適用する。
- 誤った直接upgradeで運用状態が既定値に戻る事故を防ぐ。

## 対象判定
- 本runbookは、`ChainState` の旧wire（72バイト）から新wire（88バイト）へ移行するリリースで必須。
- 判定はリリースノートの「non-backward-compatible ChainState format change」を基準に行う。

## 事前準備（必須）
1. メンテナンス時間を確保する。
2. canister を停止する。
3. snapshot を取得し、IDを記録する。

```bash
dfx canister stop <canister_id>
dfx canister snapshot create <canister_id>
```

4. 必要データをエクスポートする（少なくとも以下）。
- 直近ブロック参照情報（tip）
- pending tx（再投入対象）
- 運用パラメータ（base_fee/min fee/mining interval/block gas limit）

## 実行手順
1. 新WASMを `upgrade` する。
2. canister を起動する。
3. 初期化された運用パラメータを管理APIで再設定する。
4. 必要に応じて pending tx を再投入する。

```bash
dfx canister install <canister_id> --mode upgrade --wasm <new_wasm_path>
dfx canister start <canister_id>
```

## 検証チェック
1. `health` で `tip_number`, `queue_len`, `block_gas_limit`, `instruction_soft_limit` を確認する。
2. `get_ops_status` で `mode`, `needs_migration`, `block_gas_limit`, `instruction_soft_limit` を確認する。
3. 小さなトランザクションを1件投入し、`auto-mine` が成功することを確認する。
4. `get_receipt` で receipt を確認し、`gas_used` が非0であることを確認する。

## ロールバック
- 異常時は直ちに停止し、snapshotへ戻す。

```bash
dfx canister stop <canister_id>
dfx canister snapshot load <canister_id> <snapshot_id>
dfx canister install <canister_id> --mode reinstall --wasm <old_wasm_path>
dfx canister start <canister_id>
```

## 注意
- 本移行では旧72バイトwireを自動読込しない。
- snapshotなしで本番適用しない。
- 既定値復帰が起きた場合は通常運転を継続せず、即時ロールバックする。
