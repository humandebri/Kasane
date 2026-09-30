# `tools/wrapper-vite`

Vite and React Router dashboard for the wrapper frontend. This directory is the canonical wrapper frontend; the older `tools/wrapper` workspace has been removed.

## Source Control Policy

- Tracked: `src/`, `src/declarations/`, `components/`, `lib/`, `tests/`, `scripts/`, `contracts/*.sol`, `README.md`, `package.json`, `package-lock.json`, `vite.config.ts`.
- Generated: `dist/`, `node_modules/`, `test-results/`, `tsconfig.tsbuildinfo`, `target/`, `contracts/cache/`, `contracts/out/`.
- Local only: `.env.local`.

`src/declarations/` contains tracked bindings generated from canister DID files. Update them with `npm run bindgen`. Rust E2E and scripts load Foundry artifacts from `contracts/out/`, so run `forge build` first when needed.

## Scope

- Oisy and MetaMask wallet modal.
- MetaMask unwrap transaction flow through the Kasane testnet RPC.
- Wrap submit through `quote_wrap_request` followed by `submit_wrap_request`.
- Amount-centered UI with advanced inputs.
- Request dispatch/execution status lookup.
- Status modal restoration through `/requests/:requestId`.

History persistence and external datastores are outside the current scope.

## Setup

```bash
cd tools/wrapper-vite
npm install
npm run bindgen
cp .env.example .env.local
npm run test:local:preflight
```

## Generated Bindings

- `npm run bindgen`: regenerate `src/declarations/` from `crates/ic-evm-gateway/evm_canister.did`.
- `npm run bindgen:check`: verify tracked bindings match the current DID.
- `test:local:preflight` runs `bindgen:check`.

## Environment

Create `.env.local` from `.env.example`.

- `VITE_IC_HOST`: for example `http://127.0.0.1:8000` or `https://icp-api.io`.
- `VITE_ICP_TOKEN_LIST_URL`: token list JSON URL.
- `VITE_KASANE_EVM_CANISTER_ID`: Kasane EVM canister id.
- `VITE_WRAP_CANISTER_ID`: wrap canister id.
- `VITE_EVM_WRAP_FACTORY`: 20-byte EVM factory address.
- `VITE_KASANE_RPC_URL`: Kasane RPC URL used by MetaMask unwrap.
- `VITE_KASANE_CHAIN_ID`: chain id used by MetaMask unwrap.
- `VITE_KASANE_CHAIN_NAME`: network name for `wallet_addEthereumChain`.
- `VITE_KASANE_NATIVE_CURRENCY_SYMBOL`: native currency symbol.
- `VITE_KASANE_BLOCK_EXPLORER_URL`: explorer base URL for transaction links.

`fetchRootKey` is enabled automatically when `VITE_IC_HOST` is `localhost` or `127.0.0.1`.

Example:

```bash
cat > .env.local <<'EOF'
VITE_IC_HOST=https://icp-api.io
VITE_ICP_TOKEN_LIST_URL=/icp-token-list.sample.json
VITE_KASANE_EVM_CANISTER_ID=4c52m-aiaaa-aaaam-agwwa-cai
VITE_WRAP_CANISTER_ID=lpuz5-uyaaa-aaaam-ah4da-cai
VITE_EVM_WRAP_FACTORY=0x9057eb7d9095e5e0ff2091b8870c753fb16d3ebb
VITE_KASANE_RPC_URL=https://rpc-testnet.kasane.network
VITE_KASANE_CHAIN_ID=4801360
VITE_KASANE_CHAIN_NAME=Kasane
VITE_KASANE_NATIVE_CURRENCY_SYMBOL=ICP
VITE_KASANE_BLOCK_EXPLORER_URL=https://explorer-testnet.kasane.network
EOF
```

## Cloudflare Pages

Use these settings:

- Root directory: `tools/wrapper-vite`
- Build command: `npm run build`
- Build output directory: `dist`

SPA fallback is configured in `public/_redirects`.

## Routes

- `/`: Wrap / Unwrap Console.
- `/requests/:requestId`: opens the matching request status modal.

## Authentication

- Wallet UI supports Oisy and MetaMask.
- Oisy requires the first returned account to be the default account (no subaccount, or a 32-byte zero subaccount). Nondefault or malformed subaccounts are rejected because the gateway's ledger pulls use the default account.
- `wrap`, `retry`, and `withdraw` canister actions stay disabled until the Kasane canister exposes the required `ICRC-21` support.
- Gateway consent supports `submit_wrap_request`, `submit_native_deposit`, `retry_request`, `retry_native_deposit`, `retry_native_withdrawal`, and `recover_failed_wrap`, in addition to `submit_ic_tx`. Consent is available anonymously and describes signed arguments; it does not reserve a quote or guarantee asynchronous completion.
- Automated tests exercise the installed SignerAgent with the actual Candid actor, check the selected account, target, method, and argument bytes, and reject each operation before execution. PocketIC checks anonymous consent leaves ledger balances unchanged before wrap/unwrap execution. These checks do not replace a real Oisy approval/cancellation smoke after deployment.
- Additional tests reject popup failures, altered signer response fields and invalid certificates; malformed/truncated Candid, noncanonical calldata and numeric overflow; and anonymous asset updates. PocketIC checks fee/gas caps and fee-ledger changes before debit, and refund to the original depositor exactly once after a dropped mint. Ledger fixtures use a separate minting account and start gateway collateral at zero, so actual transfers and transfer fees are exercised.
- For that smoke, use a controlled account and small amounts: verify both ledger IDs, raw base-unit amount, EVM recipient and fee caps in the wallet; cancel and confirm balances/request state stay unchanged; then approve and confirm the ledger transfer and EVM credit/mint. Check recovery against the original request and recipient rather than treating wallet approval as completion.
- Unwrap currently uses MetaMask on Kasane testnet (`chain_id=4801360`) through `eth_sendTransaction`.
- MetaMask unwrap tracks transaction hash, not request id.

## Tests

```bash
npm test
npm run lint
npm run build
npm run test:e2e:install
npm run test:e2e
```

## Playwright E2E

Configuration: `playwright.config.ts`.

Covered:

- initial console render
- wallet modal connector list
- status modal restoration through `/requests/:requestId`

Wallet connection and live MetaMask unwrap submission remain manual smoke checks.
