# Schema v3 Migration Runbook

## 目的
- schema v3移行を tick で安全に進める。
- 途中停止時もカーソルから再開できる状態を維持する。

## 事前準備（必須）
1. 対象canisterを停止可能なメンテナンス時間を確保する。
2. スナップショットを取得する。

```bash
dfx canister snapshot create <canister_id>
```

3. 取得した snapshot ID を運用記録に残す。

## 実行
1. canister を停止する。
2. snapshot を取得する。
3. 新バイナリへ upgrade する。
4. canister を起動する。
5. `get_ops_status` で `needs_migration` を確認する。
6. write系APIが拒否されること（`ops.write.needs_migration`）を確認する。
7. 通常トラフィック下で migration tick が進み、`needs_migration=false` になるまで監視する。
8. 完了後、dual-store の active 先が v3（`tx_locs_v3`）へ切替済みであることを確認する（Verify成功後にのみ切替）。
9. from_version>=3 の再実行時はコピーを省略し、active は維持される。

```bash
dfx canister stop <canister_id>
dfx canister snapshot create <canister_id>
dfx canister install <canister_id> --mode upgrade --wasm target/wasm32-unknown-unknown/release/ic_evm_wrapper.wasm.gz
dfx canister start <canister_id>
```

## 異常時復旧
- 移行が `Error` で停止、または検証不整合が出た場合は復旧を優先する。
- canister停止後、取得済みsnapshotへ戻し、旧WASMを再インストールして起動する。

```bash
dfx canister stop <canister_id>
dfx canister snapshot load <canister_id> <snapshot_id>
dfx canister install <canister_id> --mode reinstall --wasm <old_wasm_path>
dfx canister start <canister_id>
```

- 復旧後は原因分析を行い、再upgrade前に再検証する。

## 注意
- snapshot復旧は状態を巻き戻すため、復旧時点以降の書き込みは失われる。
- 本番では snapshot 取得なしで schema 移行を実施しない。
