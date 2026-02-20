# Explorer (Phase2.1)

`tools/indexer` の Postgres を読み取り、`head / blocks / tx / receipt` と運用向け集計を表示する Explorer です。
公開向け導線として `address / principal / logs / ops` を提供します。

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

Verifyを有効化する場合は以下も設定します。

```env
EXPLORER_VERIFY_ENABLED=1
EXPLORER_VERIFY_AUTH_HMAC_KEYS=kid1:replace_me
EXPLORER_VERIFY_ADMIN_USERS=user1
EXPLORER_VERIFY_ALLOWED_COMPILER_VERSIONS=0.8.30
EXPLORER_VERIFY_DEFAULT_CHAIN_ID=0
```

## 起動

```bash
npm run dev
```

- Home: `http://localhost:3000/`
- Search: `/search?q=...`
- Block: `/blocks/:number`
- Tx: `/tx/:hash`
- Address: `/address/:hex`（20-byte hex, `Transactions`/`Token Transfers` タブ）
- Principal: `/principal/:text`
- Logs: `/logs`
- Ops: `/ops`
- Verify guide: `/verify`

Block詳細ページは RPC が返す場合に `Gas Used / Gas Limit / Base Fee Per Gas / Burnt Fees / Gas vs Target` を表示します。

Home の `Latest Blocks` は、`?blocks=<N>`（1-500）で表示件数を一時変更できます。
例: `/?blocks=100`
`EXPLORER_LATEST_BLOCKS` は初期件数として使われます。

Search の入力判定:
- block number（10進） -> `/blocks/:number`
- tx hash（32-byte hex） -> `/tx/:hash`
- address（20-byte hex） -> `/address/:hex`
- principal text -> `/principal/:text`

## 事前条件

- `tools/indexer` が同じ canister を同期済みであること
- `EXPLORER_DATABASE_URL` が indexer の Postgres を指すこと

## 既知の制約

- Addressページは snapshot 情報（balance / nonce / code）に加えて tx履歴と ERC-20 Transfer 履歴を表示します。
- Addressページの tx履歴は `Transaction Hash / Method(selector推定) / Block / Age / From / Direction / To / Amount / Txn Fee` を表示します。
- address履歴は `Older`（50件単位カーソル）で継続取得します。
- token transfer履歴も `Older`（50件単位カーソル）で継続取得します。
- `/tx` の `Value / Transaction Fee` は wei由来の値を `ICP` 表記で、`Gas Price` は `effective_gas_price` を `Gwei` 表記で表示します。
- token metadata（symbol/decimals）は in-memory キャッシュを使用します（上限1000、成功TTL 24h、失敗TTL 5m、同時取得上限5）。
- Failed Transactions は `txs.receipt_status=0` を表示します（同ページ内履歴のみ）。
- Receiptページは `Timeline` を表示しますが、logs再構成であり内部call traceではありません。
- `Timeline` は Aave（v2/v3/v3 simple）/Uniswap/ERC20 の主要イベントを優先判定し、デコード不能イベントは `unknown` として表示します。
- `repay_candidate` 判定は「同一tx内で先に観測された flash borrow と同一 pool + 同一token への ERC20 transfer」を対象にします。
- `Timeline` は raw単位表示です（token decimalsを使った正規化は未対応）。
- Principalルートは導出EVM addressの `/address/:hex` へリダイレクトします（表示はAddressページに統合）。
- Principal導出は `@dfinity/ic-pub-key@1.0.1` を固定利用しています（導出互換性の安定化）。
- Verifyは `POST /api/verify/submit`（認証必須）で投入し、`GET /api/verify/status?id=...`（認証必須）で状態確認します。
- `GET /api/verify/status` は同一Bearerトークンでポーリングできます（statusではJTIを消費しません）。
- Verify重複判定は `submitted_by + input_hash` のユーザー単位です。同一入力でも別ユーザーは別requestになります。
- 公開参照は `GET /api/contracts/:address/verified`（`chainId`クエリ任意）を利用します。
- 公開参照APIは `abi_json` が壊れていても 200 を返し、`abi: null` と `abiParseError: true` を返します。
- deploy直後の自動投入は `npm run verify:submit` を使います（`VERIFY_PAYLOAD_FILE` にJSONを渡す）。
- Verifyワーカー起動前に `npm run verify:preflight` を実行し、allowlist全 `solc-<version>` の存在を確認します。
- Verifyワーカーは `npm run verify:worker` で起動します（indexer同期処理とは分離）。
- 運用手順（鍵ローテーション / preflight / jti掃除）は `/Users/0xhude/Desktop/ICP/Kasane/docs/ops/verify_runbook.md` を参照してください。
- Verifyメトリクスは固定サンプル窓（`EXPLORER_VERIFY_METRICS_SAMPLE_INTERVAL_MS`）で集計します。ワーカー再起動直後は見え方が揺れるため、アラートは緩めの閾値から開始してください。
- Logsページは canister を直接呼び出します。`topic1` / `topics OR配列` は未対応（指定時はURL正規化で除外）、`blockHash` は未対応です。
- Logsページは未指定時に `window`（既定20）で最新ブロック範囲を自動検索します（例: `/logs?window=50`）。
- Logsページの取得件数は1ページ100件固定です（`Older` で継続取得）。
- Logs検索条件は Enter または入力欄フォーカスアウト時にURLクエリへ反映されます（入力中は反映しません）。
- `rpc_eth_get_logs_paged` の制約により、`from/to` span 上限・cursor継続が必要なケースがあります。
- Tx詳細ページは `Monitor State` を内包し、`send受理` と `receipt.status` の差を明示します。
- Opsページの failure_rate は `Δdropped / max(Δsubmitted,1)`、pending stall は「15分連続で queue_len>0 かつ Δincluded=0」です。
- Opsページは cycles 時系列を `24h / 7d` 切り替えでライン表示し、`Ops Timeseries` は直近10件を表示します。
- Opsページは `ops_metrics_samples.pruned_before_block` から `Prune History (latest 10 changes)` を表示します。
- Opsページの prune 情報は `meta.prune_status` が無い環境では `not available` と表示します。
- Opsページは `Canister Capacity` で `estimated/high/low/hard_emergency` のMB表示・使用率・容量推移（estimated/high/hard）に加え、24h/7d増加率から `days_to_high_water` / `days_to_hard_emergency` を表示します。

## 内部構成（lib層）

- `lib/data.ts`: page向けのユースケース集約（home/block/tx/address/ops/principal）
- `lib/data_address.ts`: address履歴の変換・方向判定・カーソル処理
- `lib/data_ops.ts`: prune_statusパース、ops時系列計算、stall判定
- `lib/db.ts`: Postgres読み取りクエリ（txs/token_transfers/blocks/meta/metrics/ops_samples）
- `lib/rpc.ts`: canister queryのIDL定義とRPC呼び出し
- `lib/logs.ts`: `/logs` 用のフィルタ解釈・cursor処理・エラー正規化
- `lib/tx_timeline.ts`: receipt logs のイベント再構成（Aave/Uniswap/ERC20）
- `lib/tx-monitor.ts`: `send受理` と `receipt.status` を分離した状態判定（`/tx` で利用）
- `lib/principal.ts`: principal -> EVM address 導出（`@dfinity/ic-pub-key`）
- `lib/search.ts`: Search入力のルーティング判定
- `lib/verify/*`: verify入力正規化、コンパイル照合、Sourcify補助照合

## スクリプト

```bash
npm run test   # utility + db の単体テスト
npm run lint   # TypeScript型検査
npm run build  # Next.js本番ビルド
npm run verify:preflight  # verify実行前のsolc可用性チェック
npm run verify:submit  # deploy直後のverify submit
npm run verify:worker  # verify非同期ワーカー
```

### deploy直後の自動verify submit（例）

```bash
cat > /tmp/verify_payload.json <<'JSON'
{
  "chainId": 0,
  "contractAddress": "0x0123456789abcdef0123456789abcdef01234567",
  "compilerVersion": "0.8.30",
  "optimizerEnabled": true,
  "optimizerRuns": 200,
  "evmVersion": null,
  "sourceBundle": {
    "contracts/MyContract.sol": "pragma solidity ^0.8.30; contract MyContract {}"
  },
  "contractName": "MyContract",
  "constructorArgsHex": "0x"
}
JSON

VERIFY_SUBMIT_URL=http://localhost:3000/api/verify/submit \
VERIFY_PAYLOAD_FILE=/tmp/verify_payload.json \
VERIFY_AUTH_KID=kid1 \
VERIFY_AUTH_SECRET='replace_me' \
VERIFY_AUTH_SUB=deploy-bot \
npm run verify:submit
```
