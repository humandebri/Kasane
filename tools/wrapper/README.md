# tools/wrapper

Next.js + Tailwind + shadcn で構築した wrapper 用 Dashboard です。

## スコープ

- II / Oisy ウォレット接続（ブラウザ署名）
- Unwrap tx発行（`submit_ic_tx` を client から直接実行）
- Wrap submit（`submit_wrap_request` を wallet から直接実行）
- Amount中心UI + Advanced入力（asset/recipient/evm/gas/nonce）
- request_id の送信前プレビュー
- Wrap fee見積（`cycle + gas` をICPで前払い）
- allowance不足時のみ approve（asset / ICP fee）
- request_id の dispatch / execution 状態照会
- status自動ポーリング（2秒、終端状態で停止）
- status自動ポーリング失敗時は3回で自動停止
- mint失敗 request の `withdraw_failed_wrap` 実行
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
- `WRAP_CANISTER_ID`: wrap canister id
- `FETCH_ROOT_KEY`: `true`/`false`（local は `true` 推奨）

## API Route

- `GET /api/health`
  - canister 疎通（gateway / wrap）
  - 有効な設定値

## UIフロー

1. Header で `Connect II` / `Connect Oisy`
2. Wrap/Unwrap タブで amount を入力して送信
3. 詳細値は `Advanced` で必要時のみ編集
4. 送信後は status パネルが自動追跡
5. `mint_failed_recoverable=true` の場合のみ `Withdraw Failed Wrap` が有効
6. 設定不足時（`config.missing:*`）はUI上で明示し、送信を無効化

## Approve仕様

- allowance が十分な場合、`icrc2_approve` は呼びません。
- allowance 不足時のみ approve を実行します。
- `asset_id == fee_ledger` の場合、asset+fee を合算して1回の approve で処理します。
- `asset_id` は自動補完しません。Advancedで明示指定します。

## テスト

```bash
npm test
npm run lint
npm run build
```
