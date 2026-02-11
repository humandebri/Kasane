# Explorer (Phase2.1)

`tools/indexer` の Postgres を読み取り、`head / blocks / tx / receipt` と運用向け集計を表示する Explorer です。

現在のUI基盤:
- Next.js App Router
- Tailwind CSS v4
- shadcn/ui スタイルのコンポーネント（手動導入）

## セットアップ

```bash
cd tools/explorer
npm install
cp .env.example .env.local
```

`.env.local` に最低限 `EXPLORER_DATABASE_URL` と `EVM_CANISTER_ID` を設定してください。

## 起動

```bash
npm run dev
```

- Home: `http://localhost:3000/`
- Search: `/search?q=...`
- Block: `/blocks/:number`
- Tx: `/tx/:hash`
- Receipt: `/receipt/:hash`

## 事前条件

- `tools/indexer` が同じ canister を同期済みであること
- `EXPLORER_DATABASE_URL` が indexer の Postgres を指すこと

## スクリプト

```bash
npm run test   # utility + db の単体テスト
npm run lint   # TypeScript型検査
npm run build  # Next.js本番ビルド
```
