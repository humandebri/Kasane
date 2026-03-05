// どこで: unwrap request_id導出 / 何を: precompile互換ABIとkeccak計算を実装 / なぜ: submit時にrequest_idを即時提示するため

import { Principal } from "@dfinity/principal";
import { keccak_256 } from "@noble/hashes/sha3";

export const WRAP_PRECOMPILE_ADDRESS = Uint8Array.from([
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x01,
]);

type UnwrapEncodeInput = {
  callerEvmAddress: Uint8Array;
  vaultCanisterId: string;
  assetId: string;
  amount: bigint;
  recipient: string;
  userNonce: bigint;
  deadline: bigint;
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

function encodeU64Word(value: bigint, err: string): Uint8Array {
  if (value < 0n || value > 0xffff_ffff_ffff_ffffn) {
    throw new Error(err);
  }
  return bigintToWord(value);
}

function principalBytes(text: string): Uint8Array {
  return Principal.fromText(text).toUint8Array();
}

function encodeDynamicBytes(data: Uint8Array): Uint8Array {
  const lenWord = bigintToWord(BigInt(data.length));
  const paddedLen = Math.ceil(data.length / 32) * 32;
  const out = new Uint8Array(32 + paddedLen);
  out.set(lenWord, 0);
  out.set(data, 32);
  return out;
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

export function encodeUnwrapAbiInput(args: Omit<UnwrapEncodeInput, "callerEvmAddress">): Uint8Array {
  const vaultBytes = principalBytes(args.vaultCanisterId);
  const assetBytes = principalBytes(args.assetId);
  const recipientBytes = principalBytes(args.recipient);

  const vaultTail = encodeDynamicBytes(vaultBytes);
  const assetTail = encodeDynamicBytes(assetBytes);
  const recipientTail = encodeDynamicBytes(recipientBytes);

  const headWords = 6;
  const headSize = headWords * 32;
  const vaultOffset = bigintToWord(BigInt(headSize));
  const assetOffset = bigintToWord(BigInt(headSize + vaultTail.length));
  const recipientOffset = bigintToWord(BigInt(headSize + vaultTail.length + assetTail.length));

  return concatBytes([
    vaultOffset,
    assetOffset,
    bigintToWord(args.amount),
    recipientOffset,
    encodeU64Word(args.userNonce, "arg.user_nonce_out_of_range"),
    encodeU64Word(args.deadline, "arg.deadline_out_of_range"),
    vaultTail,
    assetTail,
    recipientTail,
  ]);
}

export function deriveRequestId(args: UnwrapEncodeInput): Uint8Array {
  assertLen(args.callerEvmAddress, 20, "arg.caller_evm_invalid");
  const abiInput = encodeUnwrapAbiInput(args);
  const hashInput = concatBytes([args.callerEvmAddress, abiInput]);
  return Uint8Array.from(keccak_256(hashInput));
}

export function toSubmitIcTxData(args: Omit<UnwrapEncodeInput, "callerEvmAddress">): Uint8Array {
  return encodeUnwrapAbiInput(args);
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
  const gasLimit = encodeU64Be(args.gasLimit, "arg.gas_limit_invalid");
  for (let i = 0; i < evmNonce.length; i += 1) {
    bytes.push(evmNonce[i] ?? 0);
  }
  for (let i = 0; i < gasLimit.length; i += 1) {
    bytes.push(gasLimit[i] ?? 0);
  }
  return Uint8Array.from(keccak_256(Uint8Array.from(bytes)));
}
