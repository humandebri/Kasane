# RPC Gateway Ops

`eth_sendRawTransaction` の戻り値だけでは実行成功は保証されないため、`eth_getTransactionReceipt.status` 監視を本番運用に組み込みます。

## 1. 手動実行

```bash
cd tools/rpc-gateway
EVM_RPC_URL="https://rpc-testnet.kasane.network" \
  ./ops/watch_receipt.sh 0x<tx_hash> 180 1500
```

- 成功条件: `status == 0x1`
- 失敗条件: `status != 0x1` / timeout / RPC error

## 2. 失敗通知（任意）

`ALERT_WEBHOOK_URL` を設定すると、監視失敗時に JSON をPOSTします。

```bash
ALERT_WEBHOOK_URL="https://example.com/webhook" \
EVM_RPC_URL="https://rpc-testnet.kasane.network" \
./ops/watch_receipt.sh 0x<tx_hash> 180 1500
```

## 3. systemd テンプレート（oneshot）

`receipt-watch@.service` は tx hash をインスタンス名に取る oneshot です。

配置例（VPS）:

```bash
sudo cp ops/receipt-watch@.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl start 'receipt-watch@0x<tx_hash>.service'
sudo systemctl status 'receipt-watch@0x<tx_hash>.service'
```

運用フロー:
1. 送信処理が tx hash を取得
2. 取得後すぐ `receipt-watch@<tx_hash>.service` を起動
3. 失敗時は `journalctl -u receipt-watch@<tx_hash>.service` で確認

## 4. 起動元の固定（推奨）

送信系ジョブからの起動を統一するため、補助スクリプト `start_receipt_watch.sh` を使います。

```bash
cd /opt/kasane/tools/rpc-gateway
./ops/start_receipt_watch.sh 0x<tx_hash>
```

- tx hash は `0x` + 64hex のみ受け付けます。
- スクリプトは `systemctl start receipt-watch@<tx_hash>.service` を実行し、直後の status を表示します。

## 5. 環境ファイル運用（Contabo）

`/opt/kasane/tools/.../.env.local` はデプロイ同期で消える可能性があるため、systemd は `/etc/kasane/*.env` を参照します。

- `rpc-gateway.service` -> `/etc/kasane/rpc-gateway.env`
- `kasane-indexer.service` -> `/etc/kasane/indexer.env`
- `kasane-explorer.service` -> `/etc/kasane/explorer.env`
- `receipt-watch@.service` -> `/etc/default/receipt-watch`（`ALERT_WEBHOOK_URL`）
