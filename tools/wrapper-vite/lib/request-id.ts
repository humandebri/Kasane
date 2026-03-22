// どこで: wrap/unwrap の識別子・payload 生成 / 何を: precompile互換payloadとwrap request_id導出を実装 / なぜ: 送信仕様を frontend 内で統一するため

import { Principal } from "@icp-sdk/core/principal";
import { keccak_256 } from "@noble/hashes/sha3";
import { parseTokenAmount } from "./wrap-input";

export const WRAP_PRECOMPILE_ADDRESS = Uint8Array.from([
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xff, 0x00, 0x01,
]);
const UNWRAP_PAYLOAD_VERSION = 1;
const MAX_PRINCIPAL_LEN = 29;

type UnwrapEncodeInput = {
  assetId: string;
  amount: bigint;
  recipient: string;
};

type WrapRequestIdInput = {
  // submit_wrap_request の caller principal bytes を指定する。
  fromOwner: Uint8Array;
  assetId: Uint8Array;
  amount: Uint8Array;
  evmRecipient: Uint8Array;
  evmNonce: bigint;
  gasLimit: bigint;
};

function assertLen(bytes: Uint8Array, len: number, code: string): void {
  if (bytes.length !== len) {
    throw new Error(code);
  }
}

function bigintToWord(value: bigint): Uint8Array {
  if (value < 0n) {
    throw new Error("abi.negative");
  }
  const out = new Uint8Array(32);
  let cursor = value;
  for (let idx = 31; idx >= 0 && cursor > 0n; idx -= 1) {
    out[idx] = Number(cursor & 0xffn);
    cursor >>= 8n;
  }
  if (cursor > 0n) {
    throw new Error("abi.overflow_u256");
  }
  return out;
}

function principalBytes(text: string): Uint8Array {
  return Principal.fromText(text).toUint8Array();
}

function concatBytes(parts: readonly Uint8Array[]): Uint8Array {
  const total = parts.reduce((acc, curr) => acc + curr.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

function encodeU64Be(value: bigint, code: string): Uint8Array {
  if (value < 0n || value > 0xffff_ffff_ffff_ffffn) {
    throw new Error(code);
  }
  const out = new Uint8Array(8);
  let cursor = value;
  for (let i = 7; i >= 0; i -= 1) {
    out[i] = Number(cursor & 0xffn);
    cursor >>= 8n;
  }
  return out;
}

function encodePrincipalField(text: string, code: string): Uint8Array {
  const bytes = principalBytes(text.trim());
  if (bytes.length === 0 || bytes.length > MAX_PRINCIPAL_LEN) {
    throw new Error(code);
  }
  const out = new Uint8Array(1 + MAX_PRINCIPAL_LEN);
  out[0] = bytes.length;
  out.set(bytes, 1);
  return out;
}

function hashLenPrefixed(parts: number[], bytes: Uint8Array): void {
  const len = bytes.length;
  parts.push((len >>> 24) & 0xff, (len >>> 16) & 0xff, (len >>> 8) & 0xff, len & 0xff);
  for (let i = 0; i < bytes.length; i += 1) {
    const b = bytes[i];
    if (b === undefined) {
      throw new Error("arg.byte_missing");
    }
    parts.push(b);
  }
}

export function encodeUnwrapPayload(args: Omit<UnwrapEncodeInput, "callerEvmAddress">): Uint8Array {
  return concatBytes([
    Uint8Array.from([UNWRAP_PAYLOAD_VERSION]),
    encodePrincipalField(args.assetId, "arg.asset_principal_invalid"),
    bigintToWord(args.amount),
    encodePrincipalField(args.recipient, "arg.recipient_principal_invalid"),
  ]);
}

export function toSubmitIcTxData(args: UnwrapEncodeInput): Uint8Array {
  return encodeUnwrapPayload(args);
}

export function decimalToBytes32(amountText: string): Uint8Array {
  if (!/^[0-9]+$/.test(amountText.trim())) {
    throw new Error("arg.amount_invalid");
  }
  const value = BigInt(amountText.trim());
  if (value <= 0n) {
    throw new Error("arg.amount_invalid");
  }
  return bigintToWord(value);
}

export function tokenAmountToBytes32(amountText: string, decimals: number): Uint8Array {
  return bigintToWord(parseTokenAmount(amountText, decimals, "arg.amount_invalid"));
}

export function deriveWrapRequestId(args: WrapRequestIdInput): Uint8Array {
  assertLen(args.amount, 32, "arg.amount_len_invalid");
  assertLen(args.evmRecipient, 20, "arg.evm_recipient_invalid");
  const bytes: number[] = [];
  const prefix = new TextEncoder().encode("kasane.wrap.request.v1");
  for (let i = 0; i < prefix.length; i += 1) {
    const b = prefix[i];
    if (b === undefined) {
      throw new Error("arg.prefix_invalid");
    }
    bytes.push(b);
  }
  hashLenPrefixed(bytes, args.fromOwner);
  hashLenPrefixed(bytes, args.assetId);
  hashLenPrefixed(bytes, args.amount);
  hashLenPrefixed(bytes, args.evmRecipient);
  const evmNonce = encodeU64Be(args.evmNonce, "arg.evm_nonce_invalid");
  for (let i = 0; i < evmNonce.length; i += 1) {
    bytes.push(evmNonce[i] ?? 0);
  }
  const gasLimit = encodeU64Be(args.gasLimit, "arg.gas_limit_invalid");
  for (let i = 0; i < gasLimit.length; i += 1) {
    bytes.push(gasLimit[i] ?? 0);
  }
  return Uint8Array.from(keccak_256(Uint8Array.from(bytes)));
}

export function deriveUnwrapRequestId(txId: Uint8Array, logIndex: number): Uint8Array {
  if (txId.length !== 32) {
    throw new Error("arg.tx_id_invalid");
  }
  if (!Number.isInteger(logIndex) || logIndex < 0 || logIndex > 0xffff_ffff) {
    throw new Error("arg.log_index_invalid");
  }
  const payload = new Uint8Array(36);
  payload.set(txId, 0);
  payload[32] = (logIndex >>> 24) & 0xff;
  payload[33] = (logIndex >>> 16) & 0xff;
  payload[34] = (logIndex >>> 8) & 0xff;
  payload[35] = logIndex & 0xff;
  return Uint8Array.from(keccak_256(payload));
}
