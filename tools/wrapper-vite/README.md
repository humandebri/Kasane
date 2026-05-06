# tools/wrapper-vite

Vite + React Router + Juno Function で構築した wrapper 用 Dashboard です。

このリポでは `tools/wrapper-vite` を正本として扱います。Juno 運用手順、recent requests の契約、local smoke / preflight の入口はここを基準にします。旧 `tools/wrapper` ワークスペースは削除済みです。

## ソース管理方針

- 追跡対象: `src/`, `src/declarations/`, `components/`, `lib/`, `tests/`, `scripts/`, `contracts/*.sol`, `README.md`, `package.json`, `package-lock.json`, `vite.config.ts`, `juno.config.ts`
- 生成物: `dist/`, `node_modules/`, `test-results/`, `tsconfig.tsbuildinfo`, `target/`, `contracts/cache/`, `contracts/out/`
- ローカル専用: `.env.local`

`src/satellite/index.ts` を Juno Function の正本として扱います。`src/declarations/` は canister DID / satellite definition から生成した tracked bindings を置く場所として扱い、更新は `npm run bindgen` と既存の Juno 生成フローで管理します。Rust E2E と scripts は `contracts/out/` の Foundry artifact を前提にするため、必要時は先に `forge build` を実行してください。

## スコープ

- Oisy + MetaMask の wallet modal
- Oisy principal による recent requests の参照
- MetaMask による unwrap tx発行（Kasane testnet RPC）
- Unwrap tx発行（現状は MetaMask 送信を主経路として扱う）
- Wrap submit（`quote_wrap_request` -> `submit_wrap_request` の順で実行）
- Amount中心UI + Advanced入力
- request_id の dispatch / execution 状態照会
- status モーダルを request route (`/requests/:requestId`) で再表示
- 直近履歴20件（Juno Datastore / principal ごと）
- Juno Function による health query / recent request 保存・取得

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
  - `othercanisters/wrap-canister/wrap_canister.did` と `crates/ic-evm-gateway/evm_canister.did` から `src/declarations/` を再生成します
- `npm run bindgen:check`
  - `wrap_canister` / `evm_canister` の tracked bindings が current DID と一致するか検証します
  - あわせて `juno:functions:build` を使い、`satellite` の tracked bindings が current Juno definition と一致するか検証します
- `test:local:preflight` と `test:local:wrapper:preflight` は先に `bindgen:check` を実行します
- 生成源
  - `satellite` は Juno CLI が生成します
  - `wrap_canister` / `evm_canister` は `npm run bindgen` が生成します

## 環境変数 (`.env.local`)

- `VITE_IC_HOST`: 例 `http://127.0.0.1:8000`
- `VITE_INTERNET_IDENTITY_URL`: Juno config が参照する identity provider URL
- `VITE_II_DERIVATION_ORIGIN`: Juno config が参照する derivation origin
- `VITE_KASANE_EVM_CANISTER_ID`: Kasane EVM canister id
- `VITE_WRAP_CANISTER_ID`: wrap canister id
- `VITE_EVM_WRAP_FACTORY`: 20-byte EVM factory address (`0x...`)
- `VITE_KASANE_RPC_URL`: MetaMask unwrap が使う Kasane RPC URL
- `VITE_KASANE_CHAIN_ID`: MetaMask unwrap が使う chain id
- `VITE_KASANE_CHAIN_NAME`: `wallet_addEthereumChain` 用のネットワーク名
- `VITE_KASANE_NATIVE_CURRENCY_SYMBOL`: ネイティブ通貨 symbol
- `VITE_KASANE_BLOCK_EXPLORER_URL`: tx リンク表示に使う explorer base URL
- `VITE_JUNO_SATELLITE_ID`: frontend から呼ぶ Satellite id
- `JUNO_DEV_SATELLITE_ID`: emulator/local 用 Satellite id
- `JUNO_PROD_SATELLITE_ID`: production 用 Satellite id

`fetchRootKey` は `VITE_IC_HOST` が `localhost` / `127.0.0.1` のとき自動で有効になります。

## 開発環境と本番環境の切り替え

- `npm run dev` は [`tools/wrapper-vite/.env.development`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/.env.development) を使い、local replica を前提にします
- `npm run build` は [`tools/wrapper-vite/.env.production`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/.env.production) を使い、MetaMask unwrap は Kasane testnet を前提にします
- `.env.local` は git 管理外の上書きです。手元で個別値が必要なときだけ使ってください
- local 開発では `VITE_JUNO_SATELLITE_ID` と `JUNO_DEV_SATELLITE_ID` を同じ local satellite id にしてください
- production では `VITE_JUNO_SATELLITE_ID` と `JUNO_PROD_SATELLITE_ID` を同じ production satellite id にしてください
- `npm run dev` のまま IC だけ mainnet に向けたい場合は、`.env.local` で `VITE_IC_HOST=https://icp-api.io` を上書きしてください。この構成では Juno satellite は local のままでも構いません
- development の `Manage Tokens` は [`tools/wrapper-vite/public/icp-token-list.development.json`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/public/icp-token-list.development.json) を読むため、local token `TESTICP` が再表示されます
- asset selector の `TESTICP` は local IC 向けの補助 preset です。`VITE_IC_HOST` が localhost 系のときだけ表示され、mainnet host では mainnet asset preset のみ表示されます
- ICRC-1 logo の一次情報は `icrc1_metadata` の `icrc1:logo` を使います。収集レポートは `scripts/report_icrc1_logos.sh` で `docs/ops/reports/` に保存できます

例:

```bash
cat > .env.local <<'EOF'
VITE_IC_HOST=https://icp-api.io
VITE_INTERNET_IDENTITY_URL=https://identity.ic0.app
VITE_KASANE_EVM_CANISTER_ID=4c52m-aiaaa-aaaam-agwwa-cai
VITE_WRAP_CANISTER_ID=lpuz5-uyaaa-aaaam-ah4da-cai
VITE_EVM_WRAP_FACTORY=0x9057eb7d9095e5e0ff2091b8870c753fb16d3ebb
VITE_KASANE_RPC_URL=https://rpc-testnet.kasane.network
VITE_KASANE_CHAIN_ID=4801360
VITE_KASANE_CHAIN_NAME=Kasane
VITE_KASANE_NATIVE_CURRENCY_SYMBOL=ICP
VITE_KASANE_BLOCK_EXPLORER_URL=https://explorer-testnet.kasane.network
VITE_JUNO_SATELLITE_ID=YOUR_LOCAL_SATELLITE_ID
JUNO_DEV_SATELLITE_ID=YOUR_LOCAL_SATELLITE_ID
EOF
```

## Juno prune 注意

- 2026-03-18 に公開された `@junobuild/cli` `0.14.3` で、`juno hosting prune` / `juno deploy --prune` の hot fix が入りました。
- `0.14.2` には、custom domain と Internet Identity alternative origins に必要な `/.well-known/ic-domains` と `/.well-known/ii-alternative-origins` を prune してしまう不具合がありました。
- この dashboard は [`juno.config.ts`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/juno.config.ts#L39) で `VITE_II_DERIVATION_ORIGIN` を受けるため、custom domain + II を使う運用では影響対象になり得ます。
- prune 系コマンドは、global の `juno` ではなく、以下の固定スクリプトを使ってください。

```bash
npm run juno:hosting:prune -- --mode production
npm run juno:hosting:deploy:prune -- --mode production
```

- もし `0.14.2` で `juno hosting prune` または `juno deploy --prune` を既に実行済みなら、Juno Console の custom domain wizard を再実行して metadata を再生成してください。
- alternative origins 付きの Internet Identity を使っている場合は、authentication method も再設定してください。

## ルーティング

- `/`
  - Wrap / Unwrap Console
- `/history`
  - Recent Requests 専用画面
- `/requests/:requestId`
  - console 上で該当 request の status modal を自動表示
- `/history/requests/:requestId`
  - history 画面上で該当 request の status modal を自動表示

## Juno ローカル開発

ローカル検証はまず `Skylab` emulator を起動し、その後に frontend を立ち上げます。

1. Juno CLI を入れる

```bash
npm i -g @junobuild/cli
```

2. emulator を起動する

```bash
cd tools/wrapper-vite
npm run juno:emulator:start
```

3. Console UI を開く

- `http://localhost:5866`

4. local Satellite を作り、発行された id を `.env.local` の `JUNO_DEV_SATELLITE_ID` と `VITE_JUNO_SATELLITE_ID` に入れる

5. 事前チェックを通す

```bash
npm run test:local:preflight
```

6. Juno Function を build し、frontend を起動する

```bash
npm run juno:functions:build
npm run dev
```

7. 手動スモークのガイドを表示する

```bash
npm run test:local:smoke
```

Juno local env を確認して手動スモークのガイドを表示する場合:

```bash
npm run test:local:wrapper
```

preflight まで一気に回す場合:

```bash
npm run test:local:wrapper:preflight
```

この 2 つが `tools/wrapper-vite` の Juno ローカル検証の正式入口です。

8. 作業後に emulator を止める

```bash
npm run juno:emulator:stop
```

## ローカルスモークの完了条件

- `npm run lint`
- `npm test`
- `npm run build`
- `npm run juno:functions:build`
- `/` で `Wrap / Unwrap Console` と `Connect Wallet` が表示される
- wallet modal で `Oisy` と `MetaMask` の connector tile が表示される
- `/history` 未接続時に `Connect Oisy to view request history.` が表示される
- 接続後に request 送信成功で `Recent Requests` に履歴が追加される
- リロード後も同じ principal で履歴が再取得される
- `/requests/:requestId` で status modal が再表示される
- emulator Console で `recent_requests` collection を確認できる

## wrapper-vite の Juno ローカル検証

- `npm run test:local:wrapper`
  - `tools/wrapper-vite/.env.local` の Juno local env を確認
  - `tools/wrapper-vite` の手動スモーク手順を表示
- `npm run test:local:wrapper:preflight`
  - 上記に加えて `npm test` / `npm run lint` / `npm run build` / `npm run juno:functions:build` を実行

## Juno Function

- 実装: [`src/satellite/index.ts`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/src/satellite/index.ts)
- 共通ロジック: [`lib/health.ts`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/lib/health.ts), [`lib/recent-requests.ts`](/Users/0xhude/Desktop/ICP/Kasane/tools/wrapper-vite/lib/recent-requests.ts)
- 公開 functions
- `health`
- `save_recent_request`
- `list_recent_requests`
- `recent_requests` collection には `principalText`, `requestId`, `kind`, `submittedAt` を保存します。

## 認証

- wallet UI は `Oisy` と `MetaMask` の 2 系統です
- `Recent Requests` は Oisy principal 単位で保存・参照します
- `wrap` / `retry` / `withdraw` は、Kasane `wrap_canister` に `ICRC-21` 実装が見当たらないため現状は無効化しています
- `unwrap` の実行経路は現状 `MetaMask` を前提にします
- MetaMask unwrap は Kasane testnet (`chain_id=4801360`) を前提に `eth_sendTransaction` で送信します
- MetaMask unwrap の status modal は request_id ではなく tx hash を追跡します
- custom asset の ledger を approve する場合は、その principal を `JUNO_AUTH_ALLOWED_TARGETS` に追加してください
- 例: `JUNO_AUTH_ALLOWED_TARGETS=mxzaz-hqaaa-aaaar-qaada-cai,aaaaa-aa`
- Oisy canister action を再有効化する場合は、まず Kasane canister 側の `ICRC-21` 対応可否を確認してください

health は UI から直接は参照しておらず、運用用 query として残しています。local では emulator 起動後に `npm run juno:functions:build` を実行し、function build が通ることをまず確認してください。

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
  - `/history` の未接続表示
  - `/requests/:requestId` での status modal 再表示
- wallet 接続を含む保存/再取得、MetaMask unwrap の送信確認は現段階では手動スモークで確認します。
