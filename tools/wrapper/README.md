# tools/wrapper

Next.js + Tailwind + shadcn で構築した wrapper 用 Minimal Dashboard です。

## スコープ

- unwrap 送信フォーム（BFF経由）
- request_id の単体照会（回収状態表示）
- mint失敗requestの withdraw 実行
- 直近履歴20件（セッション内メモリ）

## セットアップ

```bash
cd tools/wrapper
npm install
cp .env.example .env.local
npm run dev
```

## 環境変数 (`.env.local`)

- `NEXT_PUBLIC_IC_HOST`: 例 `http://127.0.0.1:4943`
- `EVM_GATEWAY_CANISTER_ID`: evm gateway canister id
- `WRAP_CANISTER_ID`: wrap canister id（必須）
- `ICP_IDENTITY_SECRET_KEY_HEX`: submit(update) 用 Ed25519 秘密鍵（32-byte hex）
- `FETCH_ROOT_KEY`: `true`/`false`（local は `true` 推奨）

`ICP_IDENTITY_SECRET_KEY_HEX` はサーバー側でのみ利用されます（`NEXT_PUBLIC_` を付けない）。

## API

### `POST /api/wrap/submit`

入力:

```json
{
  "assetId": "aaaaa-aa",
  "amount": "1000000000000000000",
  "recipient": "aaaaa-aa"
}
```

出力:

```json
{
  "ok": true,
  "requestId": "0x...",
  "dispatchStatus": "Queued",
  "vaultCanisterId": "aaaaa-aa"
}
```

### `GET /api/wrap/status/[requestId]`

出力:

```json
{
  "requestId": "0x...",
  "dispatchStatus": "Dispatched",
  "executionStatus": "Running",
  "vaultCanisterId": "aaaaa-aa",
  "ledgerTxId": null,
  "errorCode": null,
  "mintFailedRecoverable": false,
  "withdrawn": false,
  "withdrawLedgerTxId": null,
  "withdrawErrorCode": null
}
```

### `POST /api/wrap/withdraw`

入力:

```json
{
  "requestId": "0x..."
}
```

出力:

```json
{
  "ok": true,
  "requestId": "0x...",
  "ledgerTxId": "0x..."
}
```

### `GET /api/health`

- canister 疎通（wrapper / wrap）
- 有効な設定値

## テスト

```bash
npm test
npm run lint
```
