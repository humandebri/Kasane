# Mainnet Deploy Runbook (`ic`)

## どこで・何を・なぜ
- どこで: `evm_canister` 本番デプロイ
- 何を: `icp` CLI で preflight / deploy / verify を実施
- なぜ: 本番導線を手順化し、運用ミスを減らす

## 1. 事前確認
1. 本番用 identity を選ぶ。
2. `evm_canister` の controller 構成を確認する。
3. cycles 残高を確認する（最低目安: `2_000_000_000_000`）。

```bash
ICP_ENV=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=<identity_name> \
scripts/ic_mainnet_preflight.sh
```

## 2. デプロイ実行

既存 canister の通常更新（推奨）:

```bash
ICP_ENV=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=<identity_name> \
MODE=upgrade \
scripts/ic_mainnet_deploy.sh
```

初回 install/reinstall（`InitArgs` 必須）:

```bash
ICP_ENV=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=<identity_name> \
MODE=install \
GENESIS_PRINCIPAL_AMOUNT=1000000000000000000 \
scripts/ic_mainnet_deploy.sh
```

## 3. デプロイ後確認
1. `icp canister status -e ic <canister_id>` で module hash / settings / balance を確認する。
2. `get_ops_status` で `needs_migration=false` を確認する。
3. read 系 RPC（`rpc_eth_chain_id`, `rpc_eth_block_number`）を確認する。
4. ネイティブ通貨表示を `ICP` として扱う場合、接続先ウォレット/SDK設定を以下で統一する。
   - `nativeCurrency.symbol = "ICP"`
   - `nativeCurrency.decimals = 18`
   - `1 ICP = 10^18`（EVM最小単位）

## 4. ロールバック方針
1. snapshot を事前取得する。
2. 障害時は snapshot を load し、直前安定 wasm を reinstall する。
3. 復旧後に `get_ops_status` と read 系 RPC を再確認する。
