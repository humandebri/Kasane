// どこで: wrap gas見積もり共通
// 何を: estimateGas用 call object と mint calldata を構築
// なぜ: frontend の自動 gas 見積もりとテストで同じ仕様を使うため

import { keccak_256 } from "@noble/hashes/sha3";
import { decimalToBytes32 } from "./request-id";
import { callerEvmAddressFromPrincipalText, principalTextToBytes } from "./principal";
import { hexToBytes } from "./utils";

export type WrapEstimateCallObject = {
  to: [] | [Uint8Array];
  from: [] | [Uint8Array];
  gas: [] | [bigint];
  gas_price: [] | [bigint];
  nonce: [] | [bigint];
  max_fee_per_gas: [] | [bigint];
  max_priority_fee_per_gas: [] | [bigint];
  chain_id: [] | [bigint];
  tx_type: [] | [bigint];
  access_list: [] | [Array<{ address: Uint8Array; storage_keys: Uint8Array[] }>];
  value: [] | [Uint8Array];
  data: [] | [Uint8Array];
};

export type BuildWrapEstimateCallArgs = {
  wrapCanisterId: string;
  evmWrapFactory: string;
  assetId: string;
  amount: string;
  evmRecipient: string;
};

function assertLen(bytes: Uint8Array, len: number, code: string): void {
  if (bytes.length !== len) {
    throw new Error(code);
  }
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

function factoryMintForAssetSelector(): Uint8Array {
  return Uint8Array.from(keccak_256(new TextEncoder().encode("mintForAsset(bytes,address,uint256)")).slice(0, 4));
}

export function encodeFactoryMintForAssetCallData(args: {
  assetId: Uint8Array;
  evmRecipient: Uint8Array;
  amount: Uint8Array;
}): Uint8Array {
  assertLen(args.evmRecipient, 20, "arg.evm_recipient_invalid");
  assertLen(args.amount, 32, "arg.amount_len_invalid");
  const padded = (32 - (args.assetId.length % 32)) % 32;
  const out = new Uint8Array(4 + 32 * 4 + args.assetId.length + padded);
  let offset = 0;
  out.set(factoryMintForAssetSelector(), offset);
  offset += 4;
  out.set(u256FromU64(96), offset);
  offset += 32;
  out.set(args.evmRecipient, offset + 12);
  offset += 32;
  out.set(args.amount, offset);
  offset += 32;
  out.set(u256FromU64(args.assetId.length), offset);
  offset += 32;
  out.set(args.assetId, offset);
  return out;
}

export function buildWrapEstimateCallObject(args: BuildWrapEstimateCallArgs): WrapEstimateCallObject {
  const factory = hexToBytes(args.evmWrapFactory.trim());
  assertLen(factory, 20, "config.evm_wrap_factory_invalid");
  const assetId = principalTextToBytes(args.assetId.trim());
  const amount = decimalToBytes32(args.amount.trim());
  const evmRecipient = hexToBytes(args.evmRecipient.trim());
  const from = callerEvmAddressFromPrincipalText(args.wrapCanisterId.trim());
  const value = new Uint8Array(32);
  const data = encodeFactoryMintForAssetCallData({
    assetId,
    evmRecipient,
    amount,
  });
  return {
    to: [factory],
    from: [from],
    gas: [],
    gas_price: [],
    nonce: [],
    max_fee_per_gas: [],
    max_priority_fee_per_gas: [],
    chain_id: [],
    tx_type: [],
    access_list: [],
    value: [value],
    data: [data],
  };
}

export function validateEstimatedGasLimit(gasLimit: bigint): bigint {
  if (gasLimit <= 0n) {
    throw new Error("wrap.estimate_gas_invalid");
  }
  return gasLimit;
}
