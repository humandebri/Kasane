# tools/wrapper

Next.js + Tailwind + shadcn で構築した wrapper 用 Dashboard です。

## スコープ

- II / Oisy ウォレット接続（ブラウザ署名）
- Unwrap tx発行（`submit_ic_tx` を client から直接実行）
- Wrap submit（`submit_wrap_request` を wallet から直接実行）
- Amount中心UI + Advanced入力（asset selector/recipient/evm/gas/nonce）
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

- `NEXT_PUBLIC_IC_HOST`: 例 `http://127.0.0.1:8000`
- `NEXT_PUBLIC_INTERNET_IDENTITY_URL`: PocketIC で II を使うときだけ指定
- `KASANE_EVM_CANISTER_ID`: Kasane EVM canister id
- `WRAP_CANISTER_ID`: wrap canister id
- `EVM_WRAP_FACTORY`: 20-byte EVM factory address (`0x...`)

`fetchRootKey` は `NEXT_PUBLIC_IC_HOST` が `localhost` / `127.0.0.1` のとき自動で有効になります。

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
- `asset_id` は Advanced の selector で明示選択します。
- プリセット候補は `ICP / ckBTC / ckETH / ckUSDC` を同梱しています。
- custom asset は UI から追加し、ブラウザの `localStorage` に保存します。

## テスト

```bash
npm test
npm run lint
npm run build
```
