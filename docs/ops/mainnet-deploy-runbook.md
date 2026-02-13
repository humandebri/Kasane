# Mainnet Deploy Runbook (`ic`)

## どこで・何を・なぜ
- どこで: `evm_canister` 本番デプロイ
- 何を: `icp` CLI で preflight / deploy / verify を実施
- なぜ: 本番導線を手順化し、運用ミスを減らす

## 1. 事前確認
1. 本番用 identity を選ぶ（本リポジトリ運用では `ci-local` を使用）。
2. `evm_canister` の controller 構成を確認する。
3. cycles 残高を確認する（最低目安: `2_000_000_000_000`）。

```bash
ICP_ENV=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=ci-local \
scripts/mainnet/ic_mainnet_preflight.sh
```

## 2. デプロイ実行

既存 canister の通常更新（推奨）:

```bash
ICP_ENV=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=ci-local \
MODE=upgrade \
scripts/mainnet/ic_mainnet_deploy.sh
```

初回 install/reinstall（`InitArgs` 必須）:

```bash
ICP_ENV=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=ci-local \
MODE=install \
GENESIS_PRINCIPAL_AMOUNT=1000000000000000000 \
scripts/mainnet/ic_mainnet_deploy.sh
```

## 3. デプロイ後確認
1. `icp canister status -e ic <canister_id>` で module hash / settings / balance を確認する。
2. query は `icp canister call` ではなく `scripts/query_smoke.sh`（`agent.query` 経路）で確認する。
3. `get_ops_status` で `needs_migration=false` を確認する。
4. read 系 RPC（`rpc_eth_chain_id`, `rpc_eth_block_number`）を確認する。

```bash
NETWORK=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=ci-local \
scripts/query_smoke.sh
```
5. `scripts/query_smoke.sh` の出力をチェックする。必ずこのようなログが出ることを確認:
   - `[query-smoke] chain_id=...` → 本番 chain_id（0 以外）で IC につながっていること。
   - `[query-smoke] ops_status needs_migration=false mode=<Low|Normal|Critical> block_gas_limit=... instruction_soft_limit=... last_cycle_balance=...`
     → `needs_migration=false` を確認し、`mode` が `Low`/`Normal` であること、gas 上限や soft limit も妥当な値を取ること。
   - `[query-smoke] export_blocks ...` → `MissingData` や `ok ...` などいずれかが出ていること（`MissingData` は許容）。
   `ログが出ない/エラーなら query 経路で何か壊れているので、deploy を中断しログを添えてチームに報告する。
6. ネイティブ通貨表示を `ICP` として扱う場合、接続先ウォレット/SDK設定を以下で統一する。
   - `nativeCurrency.symbol = "ICP"`
   - `nativeCurrency.decimals = 18`
   - `1 ICP = 10^18`（EVM最小単位）

## 4. ロールバック方針
1. snapshot を事前取得する。
2. 障害時は snapshot を load し、直前安定 wasm を reinstall する。
3. 復旧後に `get_ops_status` と read 系 RPC を再確認する。

## 5. Full Method Test（任意）
`scripts/mainnet/mainnet_method_test.sh` で本番向け総合テストを実行できる。  
`FULL_METHOD_REQUIRED=1`（既定）では次が必須:

- `ETH_PRIVKEY`
- `PRUNE_POLICY_TEST_ARGS`
- `PRUNE_POLICY_RESTORE_ARGS`
- `PRUNE_BLOCKS_ARGS`
- `ALLOW_DESTRUCTIVE_PRUNE=1`
- `DRY_PRUNE_ONLY=0`

```bash
RUN_EXECUTE=1 \
FULL_METHOD_REQUIRED=1 \
ALLOW_DESTRUCTIVE_PRUNE=1 \
DRY_PRUNE_ONLY=0 \
ETH_PRIVKEY=<hex_privkey> \
PRUNE_POLICY_TEST_ARGS='<record...>' \
PRUNE_POLICY_RESTORE_ARGS='<record...>' \
PRUNE_BLOCKS_ARGS='(0:nat64, 64:nat32)' \
ICP_ENV=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=ci-local \
scripts/mainnet/mainnet_method_test.sh
```
