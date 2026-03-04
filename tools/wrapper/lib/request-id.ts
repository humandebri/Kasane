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
