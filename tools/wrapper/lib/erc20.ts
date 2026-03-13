// どこで: wrapper の EVM contract ABI helper
// 何を: factory/token の最小 calldata encode/decode をまとめる
// なぜ: unwrap 前の token 解決・allowance 確認・approve を同じ仕様で扱うため

import { keccak_256 } from "@noble/hashes/sha3";
import { principalTextToBytes } from "./principal";

function selector(signature: string): Uint8Array {
  return Uint8Array.from(keccak_256(new TextEncoder().encode(signature)).slice(0, 4));
}

function writeAddressWord(out: Uint8Array, offset: number, address: Uint8Array): void {
  if (address.length !== 20) {
    throw new Error("arg.evm_address_invalid");
  }
  out.set(address, offset + 12);
}

function u256FromU64(value: number): Uint8Array {
  const out = new Uint8Array(32);
  let cursor = BigInt(value);
  for (let idx = 31; idx >= 0 && cursor > 0n; idx -= 1) {
    out[idx] = Number(cursor & 0xffn);
    cursor >>= 8n;
  }
  return out;
}

function u256FromBigInt(value: bigint): Uint8Array {
  if (value < 0n) {
    throw new Error("arg.uint256_negative");
  }
  const out = new Uint8Array(32);
  let cursor = value;
  for (let idx = 31; idx >= 0 && cursor > 0n; idx -= 1) {
    out[idx] = Number(cursor & 0xffn);
    cursor >>= 8n;
  }
  return out;
}

export function encodeFactoryGetTokenAddressCall(assetId: string): Uint8Array {
  const assetIdBytes = principalTextToBytes(assetId.trim());
  const padded = (32 - (assetIdBytes.length % 32)) % 32;
  const out = new Uint8Array(4 + 32 * 2 + assetIdBytes.length + padded);
  let offset = 0;
  out.set(selector("getTokenAddress(bytes)"), offset);
  offset += 4;
  out.set(u256FromU64(32), offset);
  offset += 32;
  out.set(u256FromU64(assetIdBytes.length), offset);
  offset += 32;
  out.set(assetIdBytes, offset);
  return out;
}

export function decodeAddressReturnData(data: Uint8Array): Uint8Array | null {
  if (data.length !== 32) {
    return null;
  }
  const address = data.subarray(12, 32);
  return address.every((byte) => byte === 0) ? null : new Uint8Array(address);
}

export function encodeAllowanceCall(owner: Uint8Array, spender: Uint8Array): Uint8Array {
  const out = new Uint8Array(4 + 32 * 2);
  out.set(selector("allowance(address,address)"), 0);
  writeAddressWord(out, 4, owner);
  writeAddressWord(out, 36, spender);
  return out;
}

export function decodeUint256ReturnData(data: Uint8Array): bigint {
  if (data.length !== 32) {
    throw new Error("erc20.return_data_invalid");
  }
  let out = 0n;
  for (const byte of data) {
    out = (out << 8n) | BigInt(byte);
  }
  return out;
}

export function encodeApproveCall(spender: Uint8Array, amount: bigint): Uint8Array {
  const out = new Uint8Array(4 + 32 * 2);
  out.set(selector("approve(address,uint256)"), 0);
  writeAddressWord(out, 4, spender);
  out.set(u256FromBigInt(amount), 36);
  return out;
}
