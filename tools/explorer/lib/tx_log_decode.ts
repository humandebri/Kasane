// どこで: Explorer の tx log 表示補助
// 何を: 既知イベントの表示名・emitter名・decoded fields を返す
// なぜ: logs タブの Unknown / raw hex 中心表示を減らすため

import { WRAP_PRECOMPILE_ADDRESS_HEX } from "./kasane_wrap";
import { Principal } from "@dfinity/principal";

const TOPIC_KASANE_UNWRAP_REQUEST =
  "0xfaef50ddf54b1bf879718e112b8631c1ee03bdd73f37d23a4e8c372fcf6bc548";

export const EVENT_NAME_BY_TOPIC0: Record<string, string> = {
  // ERC-20
  "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef": "Transfer",
  "0x8c5be1e5ebec7d5bd14f71427d1e84f3dd0314c0f7b2291e5b200ac8c7c3b925": "Approval",

  // Uniswap V2
  "0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822": "Swap (Uniswap V2)",
  "0x4c209b5fc8ad50758f13e2e1088ba56a560dff690a1c6fef26394f4c03821c4f": "Mint (Uniswap V2)",
  "0xdccd412f0b1252819cb1fd330b93224ca42612892bb3f4f789976e6d81936496": "Burn (Uniswap V2)",
  "0x1c411e9a96e071241c2f21f7726b17ae89e3cab4c78be50e062b03a9fffbbad1": "Sync (Uniswap V2)",

  // Uniswap V3
  "0xc42079f94a6350d7e6235f29174924f928cc2ac818eb64fed8004e115fbcca67": "Swap (Uniswap V3)",

  // Aave
  "0x631042c832b07452973831137f2d73e395028b44b250dedc5abb0ee766e168ac": "FlashLoan (Aave V2)",
  "0xefefaba5e921573100900a3ad9cf29f222d995fb3b6045797eaea7521bd8d6f0": "FlashLoan (Aave V3)",
  "0xf164a7d9b7e450d8229718aed20376118864bcc756709e0fc1d0891133dd2fe8": "FlashLoanSimple (Aave V3)",

  // Kasane
  "0xa94bf39da4a70c2c017b2fba2d3561ad61058131dac439f7f640072a9f1f3e9d": "TokenDeployed",
  "0xc24c0b635304dd6d5692e0452892191032dd98f7af780e92e277fcd453a50aa0": "Minted",
  "0x4a682b31acec5ddb2ad48221befd9bbca18586865da154421ea7c72e27473d80": "Burned",
  "0xfaef50ddf54b1bf879718e112b8631c1ee03bdd73f37d23a4e8c372fcf6bc548": "KasaneUnwrapRequest",

  // Common ownership/admin patterns
  "0x8be0079c531659141344cd1fd0a4f28419497f9722a3daafe3b4186f6b6457e0": "OwnershipTransferred",
};

export type DecodedLogField = {
  label: string;
  value: string;
  kind: "address" | "principal" | "number" | "hex" | "text";
};

export type KnownLogView = {
  eventName: string;
  emitterLabel: string | null;
  fields: DecodedLogField[];
};

export function decodeKnownLog(args: {
  addressHex: string;
  topic0Hex: string | null;
  dataHex: string;
}): KnownLogView | null {
  if (
    args.addressHex === WRAP_PRECOMPILE_ADDRESS_HEX &&
    args.topic0Hex === TOPIC_KASANE_UNWRAP_REQUEST
  ) {
    return decodeKasaneUnwrapRequest(args.dataHex);
  }
  return null;
}

export function resolveEventLabel(args: {
  addressHex: string;
  topic0Hex: string | null;
  dataHex: string;
}): string {
  const knownLog = decodeKnownLog(args);
  if (knownLog) {
    return knownLog.eventName;
  }
  if (args.topic0Hex) {
    return EVENT_NAME_BY_TOPIC0[args.topic0Hex] ?? "unknown";
  }
  return "-";
}

function decodeKasaneUnwrapRequest(dataHex: string): KnownLogView | null {
  const bytes = hexToBytes(dataHex);
  if (!bytes) {
    return null;
  }
  let offset = 0;
  const asset = readLenPrefixed(bytes, offset);
  if (!asset) {
    return null;
  }
  offset = asset.nextOffset;
  const amount = readWord(bytes, offset);
  if (!amount) {
    return null;
  }
  offset = amount.nextOffset;
  const recipient = readLenPrefixed(bytes, offset);
  if (!recipient || recipient.nextOffset !== bytes.length) {
    return null;
  }
  return {
    eventName: "KasaneUnwrapRequest",
    emitterLabel: "Wrap Precompile",
    fields: [
      {
        label: "Asset ID",
        value: principalTextOrHex(asset.value),
        kind: "principal",
      },
      {
        label: "Recipient",
        value: principalTextOrHex(recipient.value),
        kind: "principal",
      },
      {
        label: "Amount (raw)",
        value: amount.value.toString(),
        kind: "number",
      },
    ],
  };
}

function hexToBytes(value: string): Uint8Array | null {
  if (!/^0x[0-9a-fA-F]*$/.test(value) || value.length % 2 !== 0) {
    return null;
  }
  return Uint8Array.from(Buffer.from(value.slice(2), "hex"));
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
  return { value: data.subarray(start, end), nextOffset: end };
}

function readWord(
  data: Uint8Array,
  offset: number
): { value: bigint; nextOffset: number } | null {
  const end = offset + 32;
  if (end > data.length) {
    return null;
  }
  let out = 0n;
  for (const byte of data.subarray(offset, end)) {
    out = (out << 8n) + BigInt(byte);
  }
  return { value: out, nextOffset: end };
}

function toHex(bytes: Uint8Array): string {
  return `0x${Buffer.from(bytes).toString("hex")}`;
}

function principalTextOrHex(bytes: Uint8Array): string {
  try {
    return Principal.fromUint8Array(Uint8Array.from(bytes)).toText();
  } catch {
    return toHex(bytes);
  }
}
