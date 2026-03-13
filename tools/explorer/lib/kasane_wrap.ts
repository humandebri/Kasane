// どこで: Explorer の wrap/unwrap 補助
// 何を: precompile / factory tx の判定と unwrap request log の解析を行う
// なぜ: selector だけでは wrap/unwrap を正しく表現できないため

import { Principal } from "@dfinity/principal";
import { toHexLower } from "./hex";
import type { ReceiptView } from "./rpc";

export const WRAP_PRECOMPILE_ADDRESS_HEX = "0x00000000000000000000000000000000ffff0001";
const WRAP_FACTORY_MINTED_TOPIC0_HEX = "0xc24c0b635304dd6d5692e0452892191032dd98f7af780e92e277fcd453a50aa0";

export type UnwrapRequestView = {
  assetIdHex: string;
  assetIdText: string | null;
  recipientHex: string;
  recipientText: string | null;
  amount: bigint;
};

export type KasaneActionView =
  | {
      kind: "wrap";
      mintedTransfers: Array<{
        tokenAddressHex: string;
        tokenSymbol: string | null;
        tokenDecimals: number | null;
        toAddressHex: string;
        amount: bigint;
      }>;
    }
  | {
      kind: "unwrap";
      request: UnwrapRequestView;
    };

export function inferKasaneMethodLabel(toHex: string | null, txSelector: Buffer | null): string | null {
  if (toHex === WRAP_PRECOMPILE_ADDRESS_HEX) {
    return "unwrap";
  }
  return null;
}

export function isConfirmedKasaneWrapTx(args: {
  toHex: string | null;
  receipt: ReceiptView | null;
  wrapFactoryHex: string | null;
}): boolean {
  if (args.receipt === null || args.wrapFactoryHex === null || args.toHex !== args.wrapFactoryHex) {
    return false;
  }
  return args.receipt.logs.some((log) => {
    const topic0 = log.topics[0];
    return topic0 !== undefined &&
      toHexLower(log.address) === args.wrapFactoryHex &&
      toHexLower(topic0) === WRAP_FACTORY_MINTED_TOPIC0_HEX;
  });
}

export function extractUnwrapRequestFromReceipt(receipt: ReceiptView): UnwrapRequestView | null {
  for (const log of receipt.logs) {
    if (toHexLower(log.address) !== WRAP_PRECOMPILE_ADDRESS_HEX || log.topics.length !== 1) {
      continue;
    }
    const decoded = decodeUnwrapLogData(log.data);
    if (!decoded) {
      continue;
    }
    return decoded;
  }
  return null;
}

function decodeUnwrapLogData(data: Uint8Array): UnwrapRequestView | null {
  let offset = 0;
  const assetId = readLenPrefixed(data, offset);
  if (!assetId) {
    return null;
  }
  offset = assetId.nextOffset;
  const amount = readWord(data, offset);
  if (!amount) {
    return null;
  }
  offset = amount.nextOffset;
  const recipient = readLenPrefixed(data, offset);
  if (!recipient) {
    return null;
  }
  offset = recipient.nextOffset;
  if (offset !== data.length) {
    return null;
  }
  return {
    assetIdHex: toHexLower(assetId.value),
    assetIdText: decodePrincipalText(assetId.value),
    recipientHex: toHexLower(recipient.value),
    recipientText: decodePrincipalText(recipient.value),
    amount: amount.value,
  };
}

function readLenPrefixed(
  data: Uint8Array,
  offset: number
): { value: Uint8Array; nextOffset: number } | null {
  const len = data[offset];
  if (len === undefined || len === 0) {
    return null;
  }
  const start = offset + 1;
  const end = start + len;
  if (end > data.length) {
    return null;
  }
  return {
    value: data.subarray(start, end),
    nextOffset: end,
  };
}

function readWord(data: Uint8Array, offset: number): { value: bigint; nextOffset: number } | null {
  const end = offset + 32;
  if (end > data.length) {
    return null;
  }
  let value = 0n;
  for (const byte of data.subarray(offset, end)) {
    value = (value << 8n) + BigInt(byte);
  }
  return {
    value,
    nextOffset: end,
  };
}

function decodePrincipalText(bytes: Uint8Array): string | null {
  try {
    return Principal.fromUint8Array(Uint8Array.from(bytes)).toText();
  } catch {
    return null;
  }
}
