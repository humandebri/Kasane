# Explorer (Phase2.1)

`tools/indexer` の Postgres を読み取り、`head / blocks / tx / receipt` と運用向け集計を表示する Explorer です。
公開向け導線として `address / principal / logs / tx-monitor / ops` を提供します。

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
- Address: `/address/:hex`（20-byte hex）
- Principal: `/principal/:text`
- Logs: `/logs`
- Tx Monitor: `/tx-monitor/:hash`
- Ops: `/ops`

Search の入力判定:
- block number（10進） -> `/blocks/:number`
- tx hash（32-byte hex） -> `/tx/:hash`
- address（20-byte hex） -> `/address/:hex`
- principal text -> `/principal/:text`

## 事前条件

- `tools/indexer` が同じ canister を同期済みであること
- `EXPLORER_DATABASE_URL` が indexer の Postgres を指すこと

## 既知の制約

- Addressページは snapshot 情報（balance / nonce / code）に加えて tx履歴を表示します。
- address履歴は `Older`（50件単位カーソル）で継続取得します。
- Failed Transactions は `txs.receipt_status=0` を表示します（同ページ内履歴のみ）。
- Principalルートは導出EVM addressの `/address/:hex` へリダイレクトします（表示はAddressページに統合）。
- Principal導出は `@dfinity/ic-pub-key@1.0.1` を固定利用しています（導出互換性の安定化）。
- Logsページは canister を直接呼び出します。`topic1` / `topics OR配列` / `blockHash` は未対応です。
- `rpc_eth_get_logs_paged` の制約により、`from/to` span 上限・page limit上限・cursor継続が必要なケースがあります。
- Tx Monitor は `send受理` と `receipt.status` を分離表示します（`included_failed` を明確化）。
- Opsページの failure_rate は `Δdropped / max(Δsubmitted,1)`、pending stall は「15分連続で queue_len>0 かつ Δincluded=0」です。
- Opsページの prune 情報は `meta.prune_status` が無い環境では `not available` と表示します。

## 内部構成（lib層）

- `lib/data.ts`: page向けのユースケース集約（home/block/tx/receipt/address/ops/principal）
- `lib/data_address.ts`: address履歴の変換・方向判定・カーソル処理
- `lib/data_ops.ts`: prune_statusパース、ops時系列計算、stall判定
- `lib/db.ts`: Postgres読み取りクエリ（txs/blocks/meta/metrics/ops_samples）
- `lib/rpc.ts`: canister queryのIDL定義とRPC呼び出し
- `lib/logs.ts`: `/logs` 用のフィルタ解釈・cursor処理・エラー正規化
- `lib/tx-monitor.ts`: `send受理` と `receipt.status` を分離した状態判定
- `lib/principal.ts`: principal -> EVM address 導出（`@dfinity/ic-pub-key`）
- `lib/search.ts`: Search入力のルーティング判定

## スクリプト

```bash
npm run test   # utility + db の単体テスト
npm run lint   # TypeScript型検査
npm run build  # Next.js本番ビルド
```
