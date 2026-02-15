# Frontend実装ガイド: ICPウォレット起点でKasane(EVM canister)を実行する

このドキュメントは、`submit_ic_tx -> (必要なら) produce_block -> get_pending/get_receipt` の実装に絞って説明します。  
前提: canister は `4c52m-aiaaa-aaaam-agwwa-cai`、API公開は `crates/ic-evm-wrapper/evm_canister.did` に準拠。

## 1. 実行モデル（最重要）

- `submit_ic_tx` は **実行ではなくキュー投入**。
- 実行確定は `produce_block`（または auto mine 有効時のタイマー）で進む。
- UIは最低でも次の状態を持つ。
  - `Submitting`
  - `Queued`
  - `Included`
  - `Dropped`
  - `Failed`

## 2. フロントで使うAPI

- `submit_ic_tx(blob) -> Result<blob, SubmitTxError>`
- `get_pending(blob tx_id) -> PendingStatusView`（query）
- `get_receipt(blob tx_id) -> Result<ReceiptView, LookupError>`（query）
- `produce_block(nat32) -> Result<ProduceBlockStatus, ProduceBlockError>`（update, 任意）

補足: 匿名呼び出しは拒否されるため、ICPウォレットで認証した identity を使って update を呼びます。

## 3. Agent/Actor 初期化（TypeScript）

```ts
import { Actor, HttpAgent } from '@dfinity/agent';
import { AuthClient } from '@dfinity/auth-client';
import { idlFactory } from './evm_canister.did';

const CANISTER_ID = '4c52m-aiaaa-aaaam-agwwa-cai';
const HOST = 'https://icp-api.io';

export async function createEvmActor() {
  const authClient = await AuthClient.create();
  const identity = await authClient.getIdentity();

  const agent = new HttpAgent({
    host: HOST,
    identity,
  });

  return Actor.createActor(idlFactory, {
    agent,
    canisterId: CANISTER_ID,
  });
}
```

## 4. `submit_ic_tx` の入力バイトを作る

`submit_ic_tx` の payload は固定レイアウトです。

- `[version:1][to:20][value:32][gas_limit:8][nonce:8][max_fee_per_gas:16][max_priority_fee_per_gas:16][data_len:4][data]`
- Big Endian
- `version = 2`

```ts
function hexToBytes(hex: string): Uint8Array {
  const normalized = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (normalized.length % 2 !== 0) {
    throw new Error('hex length must be even');
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

export type IcSyntheticTxInput = {
  toHex20: string;
  value: bigint;
  gasLimit: bigint;
  nonce: bigint;
  maxFeePerGas: bigint;
  maxPriorityFeePerGas: bigint;
  data: Uint8Array;
};

export function encodeSubmitIcTx(input: IcSyntheticTxInput): Uint8Array {
  const to = hexToBytes(input.toHex20);
  if (to.length !== 20) {
    throw new Error('to must be 20 bytes');
  }

  const dataLen = beBytes(BigInt(input.data.length), 4);
  const out = new Uint8Array(1 + 20 + 32 + 8 + 8 + 16 + 16 + 4 + input.data.length);

  let o = 0;
  out[o] = 2; // version
  o += 1;
  out.set(to, o);
  o += 20;
  out.set(beBytes(input.value, 32), o);
  o += 32;
  out.set(beBytes(input.gasLimit, 8), o);
  o += 8;
  out.set(beBytes(input.nonce, 8), o);
  o += 8;
  out.set(beBytes(input.maxFeePerGas, 16), o);
  o += 16;
  out.set(beBytes(input.maxPriorityFeePerGas, 16), o);
  o += 16;
  out.set(dataLen, o);
  o += 4;
  out.set(input.data, o);

  return out;
}
```

## 5. 送信前チェック（nonce / gas / fee）

`submit_ic_tx` は事前に以下を決めてから送るのが安全です。

- `from` 相当アドレス（caller principal から導出）
- `nonce`（`expected_nonce_by_address`）
- `gas_limit`（`rpc_eth_estimate_gas_object`）
- `max_fee_per_gas` / `max_priority_fee_per_gas`（見積り結果にバッファを加える）

注意: ここで扱う `from` は **EVMアドレス(20 bytes)**。bytes32化した Principal データは address 引数に渡せません。
導出失敗時は canister 側で reject され、`InvalidArgument`（`arg.principal_to_evm_derivation_failed`）が返ります。

```ts
import { Principal } from '@dfinity/principal';
import { chainFusionSignerEthAddressFor } from '@dfinity/ic-pub-key/signer';

function hexToBytes(hex: string): Uint8Array {
  const normalized = hex.startsWith('0x') ? hex.slice(2) : hex;
  if (normalized.length % 2 !== 0) throw new Error('invalid hex length');
  const out = new Uint8Array(normalized.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    out[i] = Number.parseInt(normalized.slice(i * 2, i * 2 + 2), 16);
  }
  return out;
}

export function deriveEvmAddressFromPrincipal(principal: Principal): Uint8Array {
  const { response } = chainFusionSignerEthAddressFor(principal);
  return hexToBytes(response.eth_address); // 20 bytes
}

function u64ToNumber(n: bigint): number {
  if (n > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error('u64 too large for JS number');
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

export async function preflightIcSynthetic(actor: {
  expected_nonce_by_address: (addr: Uint8Array) => Promise<{ Ok?: bigint; Err?: string }>;
  rpc_eth_estimate_gas_object: (call: {
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
  }) => Promise<{ Ok?: bigint; Err?: { code: number; message: string } }>;
}, input: {
  principal: Principal;
  to: Uint8Array;
  value32: Uint8Array;
  data: Uint8Array;
  chainId: bigint;
  feeHintMaxFeePerGas: bigint; // 例: 2_000_000_000n
  feeHintPriority: bigint; // 例: 1_000_000_000n
}): Promise<PreflightResult> {
  const from20 = deriveEvmAddressFromPrincipal(input.principal);

  const nonceRes = await actor.expected_nonce_by_address(from20);
  if (nonceRes.Ok === undefined) {
    throw new Error(`expected_nonce_by_address failed: ${nonceRes.Err ?? 'unknown'}`);
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
    throw new Error(`estimateGas failed: ${estRes.Err?.message ?? 'unknown'}`);
  }

  // 実運用では estimate に少し余裕を乗せる（例: +20%）。
  const gasLimit = (estRes.Ok * 12n) / 10n;
  const maxPriorityFeePerGas = input.feeHintPriority;
  const maxFeePerGas = input.feeHintMaxFeePerGas;

  // Candid nat64/nat 変換のため number を要求するラッパを使う場合に備えて検証する。
  void u64ToNumber(nonceRes.Ok);
  void u64ToNumber(gasLimit);

  return {
    from20,
    nonce: nonceRes.Ok,
    gasLimit,
    maxFeePerGas,
    maxPriorityFeePerGas,
  };
}
```

## 6. 送信から確定までの最小実装

```ts
// principal は `const principal = (await authClient.getIdentity()).getPrincipal();` で取得する。
export async function submitAndTrack(actor: {
  submit_ic_tx: (arg: Uint8Array) => Promise<{ Ok?: Uint8Array; Err?: unknown }>;
  expected_nonce_by_address: (addr: Uint8Array) => Promise<{ Ok?: bigint; Err?: string }>;
  rpc_eth_estimate_gas_object: (call: {
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
  }) => Promise<{ Ok?: bigint; Err?: { code: number; message: string } }>;
  get_pending: (txId: Uint8Array) => Promise<unknown>;
  get_receipt: (txId: Uint8Array) => Promise<{ Ok?: unknown; Err?: unknown }>;
  produce_block: (n: number) => Promise<{ Ok?: unknown; Err?: unknown }>;
}, principal: Principal) {
  const preflight = await preflightIcSynthetic(actor, {
    principal,
    to: hexToBytes('0x0000000000000000000000000000000000000001'),
    value32: beBytes(0n, 32),
    data: new Uint8Array(),
    chainId: 4_801_360n,
    feeHintMaxFeePerGas: 2_000_000_000n,
    feeHintPriority: 1_000_000_000n,
  });

  const payload = encodeSubmitIcTx({
    toHex20: '0x0000000000000000000000000000000000000001',
    value: 0n,
    gasLimit: preflight.gasLimit,
    nonce: preflight.nonce,
    maxFeePerGas: preflight.maxFeePerGas,
    maxPriorityFeePerGas: preflight.maxPriorityFeePerGas,
    data: new Uint8Array(),
  });

  const submit = await actor.submit_ic_tx(payload);
  if (!submit.Ok) {
    throw new Error(`submit failed: ${JSON.stringify(submit.Err)}`);
  }

  const txId = submit.Ok;

  // manual mining運用の場合のみ呼ぶ。
  await actor.produce_block(1);

  for (let i = 0; i < 20; i += 1) {
    const pending = await actor.get_pending(txId);
    const receipt = await actor.get_receipt(txId);

    if (receipt.Ok) {
      return {
        txId,
        pending,
        receipt: receipt.Ok,
      };
    }

    await new Promise((resolve) => setTimeout(resolve, 1_000));
  }

  throw new Error('timeout: receipt not available');
}
```

## 7. UX設計の推奨

- 1回目のボタン押下で `submit_ic_tx` のみ実行し、`tx_id` を即表示。
- 「確定を待つ」段階で `get_pending` をポーリングし、`Queued/Included/Dropped` を明示。
- `produce_block` は運用モードで分岐。
  - auto mine ON: フロントは呼ばない。
  - auto mine OFF: 管理者UI/バックエンドworkerのみ呼ぶ。
- `SubmitTxError.Rejected` の code はそのままUIに出さず、ユーザー向け文言に変換。
  - 例: `submit.nonce_too_low` -> 「nonceが古いため再作成が必要です」

## 8. 実装時の注意

- `submit_ic_tx` と `rpc_eth_send_raw_transaction` は用途が異なる。
  - ICPウォレット起点なら `submit_ic_tx` を使う。
  - EVM署名済み raw tx を投げるなら `rpc_eth_send_raw_transaction`。
- `nonce` は submit 前に `expected_nonce_by_address` で取得する。
- `gas_limit` は submit 前に `rpc_eth_estimate_gas_object` で見積もる。
- fee が低いと `submit.invalid_fee` で reject されるため、見積り時より高めの `max_fee_per_gas` を使う。
- `tx_id` と `eth_tx_hash` は同一ではない。
  - IcSynthetic の追跡キーは `tx_id`。
- `produce_block` は権限制御や運用状態で `NoOp`/`Err` になり得るため、UIで「採掘失敗」ではなく「未実行」を区別する。

## 9. 参照

- `README.md` の submit系入力仕様と運用方針
- `crates/ic-evm-wrapper/evm_canister.did` の service 定義
- `docs/api/rpc_eth_send_raw_transaction_payload.md`（submit->produce->receipt の追跡パターン）
