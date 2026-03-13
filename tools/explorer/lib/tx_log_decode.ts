// どこで: Explorer の tx log 表示補助
// 何を: 既知イベントの表示名・emitter名・decoded fields を返す
// なぜ: logs タブの Unknown / raw hex 中心表示を減らすため

import { WRAP_PRECOMPILE_ADDRESS_HEX } from "./kasane_wrap";
import { Principal } from "@dfinity/principal";

const TOPIC_KASANE_UNWRAP_REQUEST =
  "0xfaef50ddf54b1bf879718e112b8631c1ee03bdd73f37d23a4e8c372fcf6bc548";

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
