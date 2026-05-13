# tools/wrapper-vite

Vite + React Router で構築した wrapper 用 Dashboard。

このリポでは `tools/wrapper-vite` を wrapper frontend の正本として扱う。旧 `tools/wrapper` ワークスペースは削除済み。

## ソース管理方針

- 追跡対象: `src/`, `src/declarations/`, `components/`, `lib/`, `tests/`, `scripts/`, `contracts/*.sol`, `README.md`, `package.json`, `package-lock.json`, `vite.config.ts`
- 生成物: `dist/`, `node_modules/`, `test-results/`, `tsconfig.tsbuildinfo`, `target/`, `contracts/cache/`, `contracts/out/`
- ローカル専用: `.env.local`

`src/declarations/` は canister DID から生成した tracked bindings を置く場所として扱い、更新は `npm run bindgen` で管理する。Rust E2E と scripts は `contracts/out/` の Foundry artifact を前提にするため、必要時は先に `forge build` を実行する。

## スコープ

- Oisy + MetaMask の wallet modal
- MetaMask による unwrap tx発行（Kasane testnet RPC）
- Unwrap tx発行（現状は MetaMask 送信を主経路として扱う）
- Wrap submit（`quote_wrap_request` -> `submit_wrap_request` の順で実行）
- Amount中心UI + Advanced入力
- request_id の dispatch / execution 状態照会
- status モーダルを request route (`/requests/:requestId`) で再表示

履歴は一旦保存しない。外部Datastore / Cloudflare KV / D1 への永続化は現スコープ外。

## セットアップ

```bash
cd tools/wrapper-vite
npm install
npm run bindgen
cp .env.example .env.local
npm run test:local:preflight
```

## Generated bindings

- `npm run bindgen`
  - `crates/ic-evm-gateway/evm_canister.did` から `src/declarations/` を再生成する
- `npm run bindgen:check`
  - `evm_canister` の tracked bindings が current DID と一致するか検証する
- `test:local:preflight` は `bindgen:check` を実行する

## 環境変数 (`.env.local`)

- `VITE_IC_HOST`: 例 `http://127.0.0.1:8000`
- `VITE_ICP_TOKEN_LIST_URL`: token list JSON のURL
- `VITE_KASANE_EVM_CANISTER_ID`: Kasane EVM canister id
- `VITE_WRAP_CANISTER_ID`: wrap canister id（現状は `VITE_KASANE_EVM_CANISTER_ID` と同値扱い）
- `VITE_EVM_WRAP_FACTORY`: 20-byte EVM factory address (`0x...`)
- `VITE_KASANE_RPC_URL`: MetaMask unwrap が使う Kasane RPC URL
- `VITE_KASANE_CHAIN_ID`: MetaMask unwrap が使う chain id
- `VITE_KASANE_CHAIN_NAME`: `wallet_addEthereumChain` 用のネットワーク名
- `VITE_KASANE_NATIVE_CURRENCY_SYMBOL`: ネイティブ通貨 symbol
- `VITE_KASANE_BLOCK_EXPLORER_URL`: tx リンク表示に使う explorer base URL

`fetchRootKey` は `VITE_IC_HOST` が `localhost` / `127.0.0.1` のとき自動で有効になる。

## 開発環境と本番環境の切り替え

- `npm run dev` は [`tools/wrapper-vite/.env.development`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/.env.development) を使い、local replica を前提にする
- `npm run build` は [`tools/wrapper-vite/.env.production`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/.env.production) を使い、MetaMask unwrap は Kasane testnet を前提にする
- `.env.local` は git 管理外の上書き。手元で個別値が必要なときだけ使う
- `npm run dev` のまま IC だけ mainnet に向けたい場合は、`.env.local` で `VITE_IC_HOST=https://icp-api.io` を上書きする
- development の `Manage Tokens` は [`tools/wrapper-vite/public/icp-token-list.development.json`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/public/icp-token-list.development.json) を読むため、local token `TESTICP` が再表示される
- asset selector の `TESTICP` は local IC 向けの補助 preset。`VITE_IC_HOST` が localhost 系のときだけ表示され、mainnet host では mainnet asset preset のみ表示される
- ICRC-1 logo の一次情報は `icrc1_metadata` の `icrc1:logo` を使う。収集レポートは `scripts/report_icrc1_logos.sh` で `docs/ops/reports/` に保存できる

例:

```bash
cat > .env.local <<'EOF'
VITE_IC_HOST=https://icp-api.io
VITE_ICP_TOKEN_LIST_URL=/icp-token-list.sample.json
VITE_KASANE_EVM_CANISTER_ID=4c52m-aiaaa-aaaam-agwwa-cai
VITE_WRAP_CANISTER_ID=lpuz5-uyaaa-aaaam-agwwa-cai
VITE_EVM_WRAP_FACTORY=0x9057eb7d9095e5e0ff2091b8870c753fb16d3ebb
VITE_KASANE_RPC_URL=https://rpc-testnet.kasane.network
VITE_KASANE_CHAIN_ID=4801360
VITE_KASANE_CHAIN_NAME=Kasane
VITE_KASANE_NATIVE_CURRENCY_SYMBOL=ICP
VITE_KASANE_BLOCK_EXPLORER_URL=https://explorer-testnet.kasane.network
EOF
```

## Cloudflare Pages

Cloudflare Pages の Git連携では以下を指定する。

- Root directory: `tools/wrapper-vite`
- Build command: `npm run build`
- Build output directory: `dist`

SPA fallback は [`public/_redirects`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/public/_redirects) で `/* /index.html 200` を指定している。

## ルーティング

- `/`
  - Wrap / Unwrap Console
- `/requests/:requestId`
  - console 上で該当 request の status modal を自動表示

## 認証

- wallet UI は `Oisy` と `MetaMask` の 2 系統
- `wrap` / `retry` / `withdraw` は、Kasane `evm_canister` に `ICRC-21` 実装が見当たらないため現状は無効化
- `unwrap` の実行経路は現状 `MetaMask` を前提にする
- MetaMask unwrap は Kasane testnet (`chain_id=4801360`) を前提に `eth_sendTransaction` で送信する
- MetaMask unwrap の status modal は request_id ではなく tx hash を追跡する
- Oisy canister action を再有効化する場合は、まず Kasane canister 側の `ICRC-21` 対応可否を確認する

## テスト

```bash
npm test
npm run lint
npm run build
npm run test:e2e:install
npm run test:e2e
```

## Playwright E2E

- 設定: [`playwright.config.ts`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/playwright.config.ts)
- 対象:
  - console 初期表示
  - wallet modal の connector 表示
  - `/requests/:requestId` での status modal 再表示
- wallet 接続、MetaMask unwrap の送信確認は現段階では手動スモークで確認する。
