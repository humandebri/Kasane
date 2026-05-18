# RPC Gateway Ops

`eth_sendRawTransaction` return values do not guarantee execution success. Integrate `eth_getTransactionReceipt.status` monitoring in production.

This systemd flow is legacy / rollback operation after Cloudflare Workers migration. Primary production gateway traffic should use `wrangler.jsonc` and Cloudflare routes.

## 1. Manual Run

```bash
cd tools/rpc-gateway
EVM_RPC_URL="https://rpc.example.com" \
  ./ops/watch_receipt.sh 0x<tx_hash> 180 1500
```

- Success condition: `status == 0x1`
- Failure condition: `status != 0x1` / timeout / RPC error

## 2. Failure Notification (Optional)

If `ALERT_WEBHOOK_URL` is set, monitoring failures are posted as JSON.

```bash
ALERT_WEBHOOK_URL="https://example.com/webhook" \
EVM_RPC_URL="https://rpc.example.com" \
./ops/watch_receipt.sh 0x<tx_hash> 180 1500
```

## 3. systemd Template (oneshot)

`receipt-watch@.service` is a oneshot service that takes tx hash as instance name.

Template replacement is required before deployment.

### 3.1 Template Replacement (Required)

`receipt-watch@.service` is a template. Replace the following placeholders before deployment:

- `__WORKDIR__`
- `__RUN_USER__`
- `__RUN_GROUP__`
- `__RPC_URL__`
- `__WATCH_SCRIPT__`

Check for remaining placeholders:

```bash
rg -n "__WORKDIR__|__RUN_USER__|__RUN_GROUP__|__RPC_URL__|__WATCH_SCRIPT__" ops/receipt-watch@.service
```

Deploy only when the command returns zero matches.

### 3.2 Deployment Example (VPS)

```bash
sudo cp ops/receipt-watch@.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl start 'receipt-watch@0x<tx_hash>.service'
sudo systemctl status 'receipt-watch@0x<tx_hash>.service'
```

Operational flow:
1. Sending job gets tx hash
2. Start `receipt-watch@<tx_hash>.service` immediately
3. On failure, inspect `journalctl -u receipt-watch@<tx_hash>.service`

## 4. Fixed Entry Point (Recommended)

To standardize startup from sender jobs, use helper script `start_receipt_watch.sh`.

```bash
cd <PROJECT_ROOT>/tools/rpc-gateway
./ops/start_receipt_watch.sh 0x<tx_hash>
```

- tx hash must be `0x` + 64 hex characters
- script runs `systemctl start receipt-watch@<tx_hash>.service` and prints status immediately

## 5. Environment File Operation (Generic)

If `.env.local` is removed by deploy sync, prefer systemd `EnvironmentFile`.

Example:
- `rpc-gateway.service` -> `<ENV_FILE_PATH>/rpc-gateway.env`
- `receipt-watch@.service` -> `<ENV_FILE_PATH>/receipt-watch.env`
