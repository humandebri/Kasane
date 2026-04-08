# tools/wrapper-vite

Vite + React Router + Juno Function で構築した wrapper 用 Dashboard です。

このリポでは `tools/wrapper-vite` を正本として扱います。Juno 運用手順、recent requests の契約、local smoke / preflight の入口はここを基準にします。旧 `tools/wrapper` ワークスペースは削除済みです。

## ソース管理方針

- 追跡対象: `src/`, `components/`, `lib/`, `tests/`, `contracts/*.sol`, `README.md`, `package.json`, `package-lock.json`, `vite.config.ts`, `juno.config.ts`
- 生成物: `dist/`, `node_modules/`, `test-results/`, `tsconfig.tsbuildinfo`, `target/`, `contracts/cache/`, `contracts/out/`, `src/declarations/`
- ローカル専用: `.env.local`

`src/satellite/index.ts` を Juno Function の正本として扱い、`src/declarations/` の生成クライアントはソース管理しません。Rust E2E と scripts は `contracts/out/` の Foundry artifact を前提にするため、必要時は先に `forge build` を実行してください。

## スコープ

- Juno auth による Google / Internet Identity 認証
- Unwrap tx発行（`submit_ic_tx` を client から直接実行）
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
cp .env.example .env.local
npm run test:local:preflight
```

## 環境変数 (`.env.local`)

- `VITE_IC_HOST`: 例 `http://127.0.0.1:8000`
- `VITE_GOOGLE_CLIENT_ID`: Google sign-in で使う client id
- `VITE_INTERNET_IDENTITY_URL`: `https://identity.ic0.app` など Juno が解釈できる II URL
- `VITE_II_DERIVATION_ORIGIN`: custom domain から II を使う場合に指定
- `VITE_KASANE_EVM_CANISTER_ID`: Kasane EVM canister id
- `VITE_WRAP_CANISTER_ID`: wrap canister id
- `VITE_EVM_WRAP_FACTORY`: 20-byte EVM factory address (`0x...`)
- `VITE_JUNO_SATELLITE_ID`: frontend から呼ぶ Satellite id
- `JUNO_DEV_SATELLITE_ID`: emulator/local 用 Satellite id
- `JUNO_PROD_SATELLITE_ID`: production 用 Satellite id

`fetchRootKey` は `VITE_IC_HOST` が `localhost` / `127.0.0.1` のとき自動で有効になります。

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
  - Wrap / Unwrap dashboard
- `/requests/:requestId`
  - 同じ dashboard を表示しつつ、該当 request の status モーダルを自動で開く

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
- 未接続時に `Connect wallet to load history` が表示される
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

- header の主導線は `Continue with Google`
- 代替導線は `Internet Identity`
- Google callback は `/auth/callback`
- Google sign-in は `signIn({ google: { options: { redirect: ... } } })` で `/auth/callback` を指定します
- delegation は `authentication.google.delegation` として Google provider に設定します
- custom asset の ledger を approve する場合は、その principal を `JUNO_AUTH_ALLOWED_TARGETS` に追加して delegation の `allowedTargets` へ含めます
- 例: `JUNO_AUTH_ALLOWED_TARGETS=mxzaz-hqaaa-aaaar-qaada-cai,aaaaa-aa`
- wrap / unwrap 実行時の署名には Juno auth の `getIdentityOnce()` をそのまま使います

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
  - 未接続時の履歴欄
  - `/requests/:requestId` での status modal 再表示
- wallet 接続を含む保存/再取得は現段階では手動スモークで確認します。
