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

RPC互換変更を含むリリースは **canister → gateway** の順序を固定し、逆順デプロイを禁止する。

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
GENESIS_PRINCIPAL_AMOUNT=100000000000000000000000 \
scripts/mainnet/ic_mainnet_deploy.sh
```

## 3. デプロイ後確認
1. `icp canister status -e ic <canister_id>` で module hash / settings / balance を確認する。
2. query は `icp canister call` ではなく `scripts/query_smoke.sh`（`agent.query` 経路）で確認する。
3. `get_ops_status` で `needs_migration=false` を確認する。
4. read 系 RPC（`rpc_eth_chain_id`, `rpc_eth_block_number`）を確認する。
5. `rpc_eth_history_window` が応答することを確認する（gateway起動fail-fast条件）。

```bash
NETWORK=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=ci-local \
scripts/query_smoke.sh
```
6. `scripts/query_smoke.sh` の出力をチェックする。必ずこのようなログが出ることを確認:
   - `[query-smoke] chain_id=...` → 本番 chain_id（0 以外）で IC につながっていること。
   - `[query-smoke] ops_status needs_migration=false mode=<Low|Normal|Critical> block_gas_limit=... instruction_soft_limit=... last_cycle_balance=...`
     → `needs_migration=false` を確認し、`mode` が `Low`/`Normal` であること、gas 上限や soft limit も妥当な値を取ること。
   - `[query-smoke] export_blocks ...` → `MissingData` や `ok ...` などいずれかが出ていること（`MissingData` は許容）。
   `ログが出ない/エラーなら query 経路で何か壊れているので、deploy を中断しログを添えてチームに報告する。
7. fee/取り込み健全性の監視指標を確認する。
   - `drop_counts` が急増していないこと（特に code=5）。
   - `drop_counts` の code=10（`exec_precheck`）が増えた場合、nonce再同期/残高不足を優先確認する。
   - `total_submitted - total_included` が増え続けていないこと。
   - `effectiveGasPrice` が想定レンジ（運用値）に収まっていること。
   - `queue_len` が平常値に戻ること。

### RPC意味論バージョン運用
- `safe/finalized` は当面 `latest` 同義の暫定実装。
- `earliest` は block `0` 固定で評価し、`oldest_available > 0` なら必ず `invalid.block_range.out_of_window` を返す。
- `eth_getTransactionCount` の `earliest` は、保持範囲内でも historical nonce 未提供のため `exec.state.unavailable` を返す。
- `eth_getTransactionCount` の `QUANTITY` は、`head` と同値なら `latest` と同値で成功し、`head` 未満は `exec.state.unavailable`、保持範囲外は `invalid.block_range.out_of_window` を返す。
- `eth_call` / `eth_estimateGas` の `QUANTITY` は、`head` と同値なら `latest` と同値で成功し、`head` 未満は `exec.state.unavailable`、保持範囲外は `invalid.block_range.out_of_window` を返す。
- `eth_maxPriorityFeePerGas` は観測データ不足時に `0x0` へフォールバックせず、`exec.state.unavailable` を返す。
- 意味論変更時は `RPC_SEMANTICS_VERSION` を更新し、`web3_clientVersion` で識別可能にする。
- エラー監視は JSON-RPC `error.data.error_prefix` 集計を基準にし、`invalid.block_range.out_of_window` / `invalid.fee_history.*` / `exec.state.unavailable` を主要キーとして扱う。

### Dropped code 対応表
- `1`: `decode`
- `2`: `exec`
- `3`: `missing`
- `4`: `caller_missing`
- `5`: `invalid_fee`
- `6`: `replaced`
- `7`: `result_too_large`
- `8`: `block_gas_exceeded`
- `9`: `instruction_budget`
- `10`: `exec_precheck`（nonce不整合・残高不足・intrinsic gas不正などの事前検証失敗）

### `exec_precheck` 発生時の再同期手順
1. `expected_nonce_by_address(sender)` で canister 側 expected nonce を取得する。
2. `rpc_eth_get_balance(sender)` で `gas_limit * max_fee_per_gas + value` を満たすか確認する。
3. fee が `baseFee + priority` を満たすことを確認する（不足なら再見積り）。
4. 上記を満たした tx を同 sender の最新 nonce で再送し、`get_pending` が `Dropped` でないことを確認する。
5. `rebuild_pending_runtime_indexes` 実行時の挙動を理解しておく。
   - decode 破損した pending tx は黙殺されず、`Dropped(code=1)` として自己修復される。
   - 実行後に `drop_counts{code="1"}` が増えることがあるため、異常ではなく修復イベントとして扱う。
6. ネイティブ通貨表示を `ICP` として扱う場合、接続先ウォレット/SDK設定を以下で統一する。
   - `nativeCurrency.symbol = "ICP"`
   - `nativeCurrency.decimals = 18`
   - `1 ICP = 10^18`（EVM最小単位）
7. 手動 `auto-mine` の権限エラー文字列を監視で確認する。
   - 現行仕様: controller 以外は `auth.controller_required`
   - 監視アラートは `auth.controller_required` を前提に運用する

## 3.1 Contabo 運用ファイル配置（testnet共通）

`rsync --delete` 運用で `.env.local` が消える事故を防ぐため、環境変数は `/etc/kasane/*.env` に集約する。

- `rpc-gateway.service` -> `EnvironmentFile=/etc/kasane/rpc-gateway.env`
- `kasane-indexer.service` -> `EnvironmentFile=/etc/kasane/indexer.env`
- `kasane-explorer.service` -> `EnvironmentFile=/etc/kasane/explorer.env`
- `receipt-watch@.service` -> `EnvironmentFile=-/etc/default/receipt-watch`

送信監視の起動元は固定:

```bash
cd /opt/kasane/tools/rpc-gateway
./ops/start_receipt_watch.sh 0x<tx_hash>
```

運用ルール:
1. `eth_sendRawTransaction` 戻り値の tx hash を保存
2. 直後に `start_receipt_watch.sh` を実行
3. 成否判定は `receipt.status==0x1` のみを成功条件にする

### 3.1.1 systemd 定義の同期（kasane-indexer）

Contabo 側で手作業差分が残らないよう、systemd 定義はリポジトリ管理のファイルを同期する。

- `ops/systemd/kasane-indexer.service`
- `ops/systemd/kasane-indexer.service.d/10-alert.conf`
- `ops/systemd/kasane-alert@.service`
- `ops/systemd/kasane-explorer.service`
- `ops/systemd/systemd_webhook_alert.sh`

適用手順（Contabo）:

```bash
ssh contabo-deployer
cd /opt/kasane

sudo install -d -m 755 /etc/systemd/system/kasane-indexer.service.d
sudo cp ops/systemd/kasane-indexer.service /etc/systemd/system/kasane-indexer.service
sudo cp ops/systemd/kasane-indexer.service.d/10-alert.conf /etc/systemd/system/kasane-indexer.service.d/10-alert.conf
sudo cp ops/systemd/kasane-alert@.service /etc/systemd/system/kasane-alert@.service
sudo cp ops/systemd/kasane-explorer.service /etc/systemd/system/kasane-explorer.service
sudo cp ops/systemd/systemd_webhook_alert.sh /usr/local/bin/systemd_webhook_alert.sh
sudo chmod 755 /usr/local/bin/systemd_webhook_alert.sh

sudo systemctl daemon-reload
sudo systemctl disable --now kasane-indexer.service || true
sudo systemctl enable --now kasane-indexer.service
sudo systemctl enable --now kasane-explorer.service
sudo systemctl status --no-pager kasane-indexer.service
```

通知設定:
- webhook URL は `/etc/default/receipt-watch` の `ALERT_WEBHOOK_URL` を使う。
- `kasane-indexer.service` が失敗したとき、`OnFailure=kasane-alert@%n` で webhook 通知される。

## 3.2 Contabo: indexer migration再適用 + 再デプロイ手順

### 3.2.1 SSHエイリアス（ローカル）
`~/.ssh/config` に deployer 用 entry を追加しておく。

```sshconfig
Host contabo-deployer
  HostName 167.86.83.183
  User deployer
  IdentityFile ~/.ssh/id_ed25519
```

### 3.2.2 事前確認（Contabo）

```bash
ssh contabo-deployer
hostname
whoami
sudo -n whoami
```

`sudo -n whoami` が `root` で返ることを確認する。

### 3.2.3 indexer migration再適用（idempotent）

```bash
ssh contabo-deployer
cd /opt/kasane

# 本番indexer接続先の確認（/etc/kasane/indexer.env）
sudo sed -n '1,200p' /etc/kasane/indexer.env
source /etc/kasane/indexer.env

# 適用済みmigration確認
psql "$INDEXER_DATABASE_URL" -c "select id, to_timestamp(applied_at/1000) from schema_migrations order by id;"

# 009を再適用したい場合のみ migration履歴を戻す
psql "$INDEXER_DATABASE_URL" -c "delete from schema_migrations where id='009_add_txs_selector.sql';"

# indexer再起動（起動時に未適用migrationを自動適用）
sudo systemctl restart kasane-indexer.service
sudo journalctl -u kasane-indexer.service -n 200 --no-pager
```

補足:
- `009_add_txs_selector.sql` は `add column if not exists` のため、再適用しても破壊的変更にならない。
- `schema_migrations` から対象idを消しても、SQL自体が安全に再実行される前提で運用する。

### 3.2.4 canister再デプロイ（upgrade）

```bash
ssh contabo-deployer
cd /opt/kasane

source /etc/kasane/indexer.env
ICP_ENV=ic \
CANISTER_ID="${EVM_CANISTER_ID}" \
ICP_IDENTITY_NAME=ci-local \
MODE=upgrade \
CONFIRM=0 \
scripts/mainnet/ic_mainnet_deploy.sh
```

### 3.2.5 デプロイ後確認 + explorer再起動

```bash
ssh contabo-deployer
cd /opt/kasane
source /etc/kasane/indexer.env

NETWORK=ic \
CANISTER_ID="${EVM_CANISTER_ID}" \
ICP_IDENTITY_NAME=ci-local \
scripts/query_smoke.sh

sudo systemctl restart kasane-explorer.service
sudo journalctl -u kasane-explorer.service -n 200 --no-pager
```

## 4. ロールバック方針
1. snapshot を事前取得する。
2. 障害時は snapshot を load し、直前安定 wasm を reinstall する。
3. 復旧後に `get_ops_status` と read 系 RPC を再確認する。

### 4.1 Receipt API分離リリース時の同世代ロールバック
- 破壊的変更として `rpc_eth_get_transaction_receipt_with_status` を削除済みの場合、`canister/gateway/explorer/indexer` は同世代で揃えて戻す。
- 手順は「gateway/explorer/indexer 停止 -> canister downgrade -> gateway/explorer/indexer downgrade 起動」の順で実施する。
- 外部 JSON-RPC の receipt 参照は `eth_tx_hash` 専用（canister は `rpc_eth_get_transaction_receipt_with_status_by_eth_hash`）で確認する。
- 内部運用の `submit_ic_tx` 追跡は `tx_id` 系（`get_pending`/`get_receipt`）で確認する。

## 5. Full Method Test（任意）
`scripts/mainnet/mainnet_method_test.sh` で本番向け総合テストを実行できる。  
`FULL_METHOD_REQUIRED=1`（既定）では次が必須:

- `ETH_PRIVKEY` または `AUTO_FUND_TEST_KEY=1`
- `PRUNE_POLICY_TEST_ARGS`
- `PRUNE_POLICY_RESTORE_ARGS`

84-block prune cadence 前提の推奨初期値:
- `retain_blocks`: `168`
- `max_ops_per_tick`: `300`
- `retain_days`: `14`
- `target_bytes`: `0`（容量制御を使わない場合）

`prune_blocks` を本番で実行する場合のみ、以下を追加で指定する（通常は不要）:
- `ALLOW_DESTRUCTIVE_PRUNE=1`
- `DRY_PRUNE_ONLY=0`
- `PRUNE_BLOCKS_ARGS`

```bash
RUN_EXECUTE=1 \
FULL_METHOD_REQUIRED=1 \
RUN_STRICT=1 \
AUTO_FUND_TEST_KEY=1 \
AUTO_FUND_AMOUNT_WEI=500000000000000000 \
PRUNE_POLICY_TEST_ARGS='(record {
  headroom_ratio_bps = 2000:nat32;
  target_bytes = 0:nat64;
  retain_blocks = 168:nat64;
  retain_days = 14:nat64;
  hard_emergency_ratio_bps = 9500:nat32;
  max_ops_per_tick = 300:nat32;
})' \
PRUNE_POLICY_RESTORE_ARGS='(record {
  headroom_ratio_bps = 2000:nat32;
  target_bytes = 0:nat64;
  retain_blocks = 168:nat64;
  retain_days = 14:nat64;
  hard_emergency_ratio_bps = 9500:nat32;
  max_ops_per_tick = 300:nat32;
})' \
ICP_ENV=ic \
CANISTER_ID=<canister_id> \
ICP_IDENTITY_NAME=ci-local \
scripts/mainnet/mainnet_method_test.sh
```

## 6. `caller_principal` と `canister_id` の扱い
- 用語:
  - `caller_principal`: 「誰が呼び出したか（主体）」を表す情報。
  - `canister_id`: 「どの canister 文脈で生成されたか（発行ドメイン）」を表す情報。
- `IcSynthetic`:
  - `caller_principal` と `canister_id` の両方を使う。
  - どちらかが欠損すると不正データとして reject される。
  - 監査時に「誰が」「どの発行経路で」生成したかを分離して追跡できる。
- `EthSigned`:
  - 生の署名txが本体であり、`canister_id` は使わない。
  - `canister_id` が非空なら reject（型境界を守るため）。
  - `caller_principal` は運用メタデータとして保持してよいが、`tx_id` 計算には使わない。
- 運用上の注意:
  - 同一 raw tx は principal が異なっても同一 `tx_id` になる（`EthSigned` の仕様）。
  - 送信経路の切り分けが必要な調査では、`IcSynthetic` の `canister_id` を必ず併読する。

## 7. Genesis 配布メモ（本番）
- デプロイスクリプトの仕様:
  - `MODE=install` 時のみ `InitArgs.genesis_balances` を投入する。
  - 配布先は常に `build_init_args_for_current_identity(...)`（Principal由来アドレスのみ）。
  - `GENESIS_ETH_PRIVKEY` / `GENESIS_ETH_AMOUNT` は未対応（指定するとエラー）。
- 本番 canister (`4c52m-aiaaa-aaaam-agwwa-cai`) の read-only 確認結果:
  - `ci-local` Principal 由来 EVMアドレス: `2287ead6f2f95b19696e900face81857db2b701d`
  - 上記アドレスの残高: `1e18 wei`（`0x0de0b6b3a7640000`）
  - 現在のデフォルトは `GENESIS_PRINCIPAL_AMOUNT=100000000000000000000000`（100,000倍）。
- 制約:
  - canister から「genesisで配布された全アドレス一覧」や「対応秘密鍵」は取得できない。
  - したがって、任意の `privkey` で genesis 資金を利用できるわけではない（対応鍵が必要）。

## 8. Principal→EVM導出 signer 定数ローテーション（コード固定運用）
- 現行方針:
  - signer は `ic-pub-key` の `key_1` と canister id（`grghe-syaaa-aaaar-qabyq-cai`）をコード固定で利用する。
  - ローテーション時は `InitArgs` 注入や管理API更新ではなく、再デプロイで切り替える。
- 更新対象:
  - `/Users/0xhude/Desktop/ICP/Kasane/crates/ic-evm-address/src/lib.rs`
  - `CHAIN_FUSION_SIGNER_CANISTER_ID`
  - `KEY_ID_KEY_1`
- ローテーション手順:
  1. 上記定数を更新する。
  2. 既存ベクタ（`nggqm-...`）と、運用で使う代表 Principal の導出結果をテストで更新する。
  3. `cargo test --workspace` を実行し、`ic-evm-address`/`evm-core`/`evm-db` の導出回帰がないことを確認する。
  4. `scripts/mainnet/ic_mainnet_preflight.sh` を実施してから upgrade デプロイする。
  5. デプロイ後に `scripts/query_smoke.sh` と `scripts/mainnet/mainnet_method_test.sh`（必要時）で導出経路を確認する。
- 検証ポイント:
  - `submit_ic_tx` が `arg.principal_to_evm_derivation_failed` を返していないこと。
  - 20 bytes address と bytes32（Principalエンコード）が混同されていないこと。
