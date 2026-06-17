# Explorer

The explorer reads the Postgres database maintained by `tools/indexer` and displays blocks, transactions, receipts, addresses, principals, logs, ops metrics, and contract verification state.

Current stack:

- Next.js App Router
- Tailwind CSS v4
- shadcn/ui-style components added manually

## Setup

```bash
cd tools/explorer
pnpm install
cp .env.example .env.local
```

Set at least `EXPLORER_DATABASE_URL` and `EVM_CANISTER_ID` in `.env.local`.

Verification support also needs:

```env
EXPLORER_VERIFY_ENABLED=1
EXPLORER_VERIFY_AUTH_HMAC_KEYS=kid1:replace_me
EXPLORER_VERIFY_ADMIN_USERS=user1
EXPLORER_VERIFY_ALLOWED_COMPILER_VERSIONS=0.8.30
EXPLORER_VERIFY_DEFAULT_CHAIN_ID=0
```

## Run

```bash
pnpm run dev
```

Routes:

- `/`
- `/search?q=...`
- `/blocks/:number`
- `/tx/:hash`
- `/address/:hex`
- `/principal/:text`
- `/logs`
- `/ops`
- `/verify`

Search routing:

- decimal block number -> `/blocks/:number`
- 32-byte hex -> `/tx/:hash`
- 20-byte hex -> `/address/:hex`
- principal text -> `/principal/:text`

## Prerequisites

- `tools/indexer` must already be syncing the same canister.
- `EXPLORER_DATABASE_URL` must point at the indexer Postgres database.
- Logs use the RPC gateway configured by `EXPLORER_RPC_GATEWAY_URL`.

## Behavior and Limits

- Address pages show snapshot data plus `Transactions`, `Internal Transactions`, `Token Transfers`, `Contract Events`, and `Contract`.
- Direct transactions and internal traces are separate histories.
- `Contract Events` fetches logs only when that tab is selected.
- `Read Contract` and `Write Contract` are not implemented.
- Histories use `Older` pagination in 50-item cursor pages.
- Token metadata is cached in memory, capped at 1000 entries, with 24h success TTL and 5m failure TTL.
- Failed Transactions show `txs.receipt_status=0` within the page history.
- Receipt `Timeline` is reconstructed from logs, not internal call traces.
- Principal routes redirect to the derived EVM address page.
- Principal derivation is pinned to `@dfinity/ic-pub-key@1.0.1`.
- Verify submit uses `POST /api/verify/submit`; status uses `GET /api/verify/status?id=...`.
- Public verification lookup uses `GET /api/contracts/:address/verified`.
- `eth_getLogs` supports the current gateway constraints; `topic1` and topic OR arrays beyond `topic0` are not supported.
- `/logs` supports `blockHash`, but not together with `fromBlock`/`toBlock`.
- Ops failure rate is `delta(dropped) / max(delta(submitted), 1)`.
- Pending stall means queue length stays above zero while included count does not increase for 15 minutes.

## Internal Modules

- `lib/data.ts`: page-level use cases.
- `lib/data_address.ts`: address history conversion, direction, cursors.
- `lib/data_ops.ts`: prune status, ops timeseries, stall detection.
- `lib/db.ts`: Postgres reads.
- `lib/rpc.ts`: canister query IDL and RPC calls.
- `lib/logs.ts`: `/logs` filters and gateway error normalization.
- `lib/tx_timeline.ts`: receipt-log timeline reconstruction.
- `lib/tx-monitor.ts`: submit acceptance vs receipt status.
- `lib/principal.ts`: principal-to-EVM address derivation.
- `lib/search.ts`: search input routing.
- `lib/verify/*`: verification normalization and compiler matching.

## Scripts

```bash
pnpm run test
pnpm run lint
pnpm run build
pnpm run verify:preflight
pnpm run verify:submit
pnpm run verify:worker
```

## Verify Submit Example

```bash
VERIFY_SUBMIT_URL=http://localhost:3000/api/verify/submit \
VERIFY_PAYLOAD_FILE=/tmp/verify_payload.json \
VERIFY_AUTH_KID=kid1 \
VERIFY_AUTH_SECRET='replace_me' \
VERIFY_AUTH_SUB=verify-bot \
pnpm run verify:submit
```
