# tools/wrapper

Next.js + Tailwind + shadcn で構築した wrapper 用 Dashboard です。

`tools/wrapper-vite` が正本です。`tools/wrapper` は deprecated 扱いで、local smoke / preflight / 現行運用の検証対象ではありません。Juno 運用手順、local smoke、recent requests の契約変更は、まず `tools/wrapper-vite` 側を更新してください。この README は Next 固有の差分と過去実装の参照だけを補足します。

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
- 直近履歴20件（Juno satellite 設定時は永続化）

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
- `NEXT_PUBLIC_II_DERIVATION_ORIGIN`: Contabo / custom domain から II を使う場合に指定
- `NEXT_PUBLIC_JUNO_SATELLITE_ID`: recent requests を保存する Juno satellite id
- `KASANE_EVM_CANISTER_ID`: Kasane EVM canister id
- `WRAP_CANISTER_ID`: wrap canister id
- `EVM_WRAP_FACTORY`: 20-byte EVM factory address (`0x...`)

`fetchRootKey` は `NEXT_PUBLIC_IC_HOST` が `localhost` / `127.0.0.1` のとき自動で有効になります。
`EVM_WRAP_FACTORY` は監査対応後の新 factory address を設定してください。旧未稼働 factory との互換は持ちません。
custom domain で II を使う場合は、`NEXT_PUBLIC_II_DERIVATION_ORIGIN` に delegation の基準 origin を設定してください。
履歴をリロード後も残したい場合は、`NEXT_PUBLIC_JUNO_SATELLITE_ID` を設定してください。未設定でも request_id 手入力での追跡は利用できます。
このリポのローカル Juno satellite を使う場合は、`tools/wrapper-vite/.env.local` の `VITE_JUNO_SATELLITE_ID` と同じ id を設定してください。

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

## テスト

```bash
npm test
npm run lint
npm run build
```

## 正本参照

- Juno / local smoke / recent requests の運用手順:
  [`tools/wrapper-vite/README.md`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/README.md)
- wrapper を含む Juno ローカル検証の入口:
  `cd tools/wrapper-vite && npm run test:local:wrapper`
- preflight:
  `cd tools/wrapper-vite && npm run test:local:wrapper:preflight`

## Juno ローカル検証

`tools/wrapper` の recent requests は `tools/wrapper-vite` 側の Juno emulator / functions を使います。詳細手順は `tools/wrapper-vite` README を参照してください。

```bash
cd tools/wrapper-vite
npm run test:local:wrapper
```

preflight まで一気に回す場合:

```bash
cd tools/wrapper-vite
npm run test:local:wrapper:preflight
```
