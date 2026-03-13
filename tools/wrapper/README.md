# tools/wrapper

Next.js + Tailwind + shadcn で構築した wrapper 用 Dashboard です。

## スコープ

- II / Oisy ウォレット接続（ブラウザ署名）
- Unwrap tx発行（`submit_ic_tx` を client から直接実行）
- Unwrap は `get_unwrap_requirements` で preflight を取り、必要時だけ wrapped token の `approve(factory, amount)` を送る
- Unwrap payload は compact 形式で生成し、送信前に `estimate_ic_tx` で `gas_limit` / fee を見積もる
- Wrap submit（`quote_wrap_request` -> `submit_wrap_request` の順で実行）
- Wrap estimate は ledger の `icrc1_metadata` から decimals を取得して factory calldata に反映
- Amount中心UI + Advanced入力（asset selector/recipient/evm/gas/nonce）
- request_id は canister / tx receipt から確定し、送信前の手計算はしない
- Wrap fee見積は `quote_wrap_request` を一次情報にする
- allowance不足時のみ approve（asset / ICP fee）
- unwrap burn は factory allowance 前提で、token 直 burn は使わない
- request_id の dispatch / execution 状態照会（`get_unwrap_dispatch_overview` + `get_request`）
- status自動ポーリング（2秒、終端状態で停止）
- status自動ポーリング失敗時は3回で自動停止
- mint失敗 request の `recover_failed_wrap` 実行
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
`EVM_WRAP_FACTORY` は監査対応後の新 factory address を設定してください。旧未稼働 factory との互換は持ちません。

## API Route

- `GET /api/health`
  - canister 疎通（gateway / wrap）
  - 有効な設定値

## UIフロー

1. Header で `Connect II` / `Connect Oisy`
2. Wrap/Unwrap タブで amount を入力して送信
3. 詳細値は `Advanced` で必要時のみ編集
4. 送信後は status パネルが自動追跡
5. mint recoverable failure の場合のみ `Recover Failed Wrap` が有効
6. 設定不足時（`config.missing:*`）はUI上で明示し、送信を無効化

## Approve仕様

- allowance が十分な場合、`icrc2_approve` は呼びません。
- allowance 不足時のみ approve を実行します。
- `asset_id == fee_ledger` の場合、asset+fee を合算して1回の approve で処理します。
- wrap 側の mint metadata は ledger の `icrc1_metadata` を一次情報として扱います。decimals 取得に失敗した asset は submit 前に止まります。
- `asset_id` は Advanced の selector で明示選択します。
- プリセット候補は `ICP / ckBTC / ckETH / ckUSDC` を同梱しています。
- custom asset は UI から追加し、ブラウザの `localStorage` に保存します。

## テスト

```bash
npm test
npm run lint
npm run build
```
