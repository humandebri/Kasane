# Frontend Guide: Running Kasane from an ICP Wallet

This guide covers the `submit_ic_tx -> block production -> get_pending/get_receipt` path.

Assumptions:

- Canister: `4c52m-aiaaa-aaaam-agwwa-cai`
- Public API: `crates/ic-evm-gateway/evm_canister.did`

## Execution Model

- `submit_ic_tx` enqueues a transaction; it does not execute the transaction immediately.
- Execution finality is reached after automatic block production.
- UI state should distinguish at least `Submitting`, `Queued`, `Included`, `Dropped`, and `Failed`.

## Canister APIs

- `submit_ic_tx(record) -> Result<blob, SubmitTxError>`
- `get_pending(blob tx_id) -> PendingStatusView` (query)
- `get_receipt(blob tx_id) -> Result<ReceiptView, LookupError>` (query)

Anonymous update calls are rejected. Use an identity authenticated through an ICP wallet.

## Agent and Actor Setup

```ts
import { Actor, HttpAgent } from "@dfinity/agent";
import { AuthClient } from "@dfinity/auth-client";
import { idlFactory } from "./evm_canister.did";

const CANISTER_ID = "4c52m-aiaaa-aaaam-agwwa-cai";
const HOST = "https://icp-api.io";

export async function createEvmActor() {
  const authClient = await AuthClient.create();
  const identity = await authClient.getIdentity();
  const agent = new HttpAgent({ host: HOST, identity });

  return Actor.createActor(idlFactory, {
    agent,
    canisterId: CANISTER_ID,
  });
}
```

## `submit_ic_tx` Record

```ts
export type SubmitIcTxArgs = {
  to: [] | [Uint8Array];
  value: bigint;
  gas_limit: bigint;
  nonce: bigint;
  max_fee_per_gas: bigint;
  max_priority_fee_per_gas: bigint;
  data: Uint8Array;
};
```

Helpers:

```ts
function hexToBytes(hex: string): Uint8Array {
  const normalized = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (normalized.length % 2 !== 0) {
    throw new Error("hex length must be even");
  }
  const out = new Uint8Array(normalized.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Number.parseInt(normalized.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

function beBytes(value: bigint, length: number): Uint8Array {
  const out = new Uint8Array(length);
  let x = value;
  for (let i = length - 1; i >= 0; i -= 1) {
    out[i] = Number(x & 0xffn);
    x >>= 8n;
  }
  if (x !== 0n) {
    throw new Error(`value too large for ${length} bytes`);
  }
  return out;
}
```

## Preflight

Before submitting, derive the sender address, fetch the nonce, estimate gas, and choose fee values.

The `from` value is a 20-byte EVM address derived from the caller principal. Do not pass a bytes32 principal encoding as an address.

```ts
import { Principal } from "@dfinity/principal";
import { chainFusionSignerEthAddressFor } from "@dfinity/ic-pub-key/signer";

export function deriveEvmAddressFromPrincipal(principal: Principal): Uint8Array {
  const { response } = chainFusionSignerEthAddressFor(principal);
  return hexToBytes(response.eth_address);
}

function u64ToNumber(n: bigint): number {
  if (n > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error("u64 too large for JS number");
  }
  return Number(n);
}

type PreflightResult = {
  from20: Uint8Array;
  nonce: bigint;
  gasLimit: bigint;
  maxFeePerGas: bigint;
  maxPriorityFeePerGas: bigint;
};

type EstimateCall = {
  to: [] | [Uint8Array];
  gas: [] | [bigint];
  value: [] | [Uint8Array];
  max_priority_fee_per_gas: [] | [bigint];
  data: [] | [Uint8Array];
  from: [] | [Uint8Array];
  max_fee_per_gas: [] | [bigint];
  chain_id: [] | [bigint];
  nonce: [] | [bigint];
  tx_type: [] | [bigint];
  access_list: [] | [Array<{ address: Uint8Array; storage_keys: Uint8Array[] }>];
  gas_price: [] | [bigint];
};

export async function preflightIcSynthetic(actor: {
  expected_nonce_by_address: (addr: Uint8Array) => Promise<{ Ok?: bigint; Err?: string }>;
  rpc_eth_estimate_gas_object: (call: EstimateCall) => Promise<{ Ok?: bigint; Err?: { code: number; message: string } }>;
}, input: {
  principal: Principal;
  to: Uint8Array;
  value32: Uint8Array;
  data: Uint8Array;
  chainId: bigint;
  feeHintMaxFeePerGas: bigint;
  feeHintPriority: bigint;
}): Promise<PreflightResult> {
  const from20 = deriveEvmAddressFromPrincipal(input.principal);

  const nonceRes = await actor.expected_nonce_by_address(from20);
  if (nonceRes.Ok === undefined) {
    throw new Error(`expected_nonce_by_address failed: ${nonceRes.Err ?? "unknown"}`);
  }

  const estRes = await actor.rpc_eth_estimate_gas_object({
    to: [input.to],
    gas: [],
    value: [input.value32],
    max_priority_fee_per_gas: [input.feeHintPriority],
    data: input.data.length === 0 ? [] : [input.data],
    from: [from20],
    max_fee_per_gas: [input.feeHintMaxFeePerGas],
    chain_id: [input.chainId],
    nonce: [nonceRes.Ok],
    tx_type: [2n],
    access_list: [],
    gas_price: [],
  });
  if (estRes.Ok === undefined) {
    throw new Error(`estimateGas failed: ${estRes.Err?.message ?? "unknown"}`);
  }

  const gasLimit = (estRes.Ok * 12n) / 10n;
  void u64ToNumber(nonceRes.Ok);
  void u64ToNumber(gasLimit);

  return {
    from20,
    nonce: nonceRes.Ok,
    gasLimit,
    maxFeePerGas: input.feeHintMaxFeePerGas,
    maxPriorityFeePerGas: input.feeHintPriority,
  };
}
```

## Submit and Track

```ts
export async function submitAndTrack(actor: {
  submit_ic_tx: (arg: SubmitIcTxArgs) => Promise<{ Ok?: Uint8Array; Err?: unknown }>;
  expected_nonce_by_address: (addr: Uint8Array) => Promise<{ Ok?: bigint; Err?: string }>;
  rpc_eth_estimate_gas_object: (call: EstimateCall) => Promise<{ Ok?: bigint; Err?: { code: number; message: string } }>;
  rpc_eth_block_number: () => Promise<unknown>;
  get_pending: (txId: Uint8Array) => Promise<unknown>;
  get_receipt: (txId: Uint8Array) => Promise<{ Ok?: unknown; Err?: unknown }>;
}, principal: Principal) {
  const to = hexToBytes("0x0000000000000000000000000000000000000001");
  const preflight = await preflightIcSynthetic(actor, {
    principal,
    to,
    value32: beBytes(0n, 32),
    data: new Uint8Array(),
    chainId: 4_801_360n,
    feeHintMaxFeePerGas: 2_000_000_000n,
    feeHintPriority: 1_000_000_000n,
  });

  const submit = await actor.submit_ic_tx({
    to: [to],
    value: 0n,
    gas_limit: preflight.gasLimit,
    nonce: preflight.nonce,
    max_fee_per_gas: preflight.maxFeePerGas,
    max_priority_fee_per_gas: preflight.maxPriorityFeePerGas,
    data: new Uint8Array(),
  });
  if (!submit.Ok) {
    throw new Error(`submit failed: ${JSON.stringify(submit.Err)}`);
  }

  const txId = submit.Ok;

  for (let i = 0; i < 20; i += 1) {
    await actor.rpc_eth_block_number();
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }

  for (let i = 0; i < 20; i += 1) {
    const pending = await actor.get_pending(txId);
    const receipt = await actor.get_receipt(txId);
    if (receipt.Ok) {
      return { txId, pending, receipt: receipt.Ok };
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }

  throw new Error("timeout: receipt not available");
}
```

## UX Guidance

- On the first click, run only `submit_ic_tx` and show `tx_id` immediately.
- Poll `get_pending` and show `Queued`, `Included`, or `Dropped` explicitly.
- In automatic mining mode, the frontend should not call administrative mining methods.
- Map `SubmitTxError.Rejected.code` values to user-facing text instead of showing raw internal codes.

## Notes

- Use `submit_ic_tx` for ICP-wallet originated transactions.
- Use `rpc_eth_send_raw_transaction` for signed Ethereum raw transactions.
- Get nonce with `expected_nonce_by_address` before submit.
- Estimate gas with `rpc_eth_estimate_gas_object` before submit.
- Keep fee values above the estimate to avoid `submit.invalid_fee`.
- `tx_id` and `eth_tx_hash` are different.
