// どこで: RPC変換層 / 何を: DATA/QUANTITY hex規約を実装 / なぜ: JSON-RPC互換の地雷を避けるため

const HEX = /^0x[0-9a-fA-F]*$/;

export function toDataHex(bytes: Uint8Array): string {
  return `0x${Buffer.from(bytes).toString("hex")}`;
}

export function parseDataHex(value: string): Uint8Array {
  if (!HEX.test(value)) {
    throw new Error("DATA must be 0x-prefixed hex");
  }
  const body = value.slice(2);
  if (body.length % 2 !== 0) {
    throw new Error("DATA hex length must be even");
  }
  return Uint8Array.from(Buffer.from(body, "hex"));
}

export function toQuantityHex(value: bigint): string {
  if (value < 0n) {
    throw new Error("QUANTITY must be non-negative");
  }
  if (value === 0n) {
    return "0x0";
  }
  return `0x${value.toString(16)}`;
}

export function parseQuantityHex(value: string): bigint {
  if (!/^0x[0-9a-fA-F]+$/.test(value)) {
    throw new Error("QUANTITY must be 0x-prefixed hex without sign");
  }
  if (value !== "0x0" && value.startsWith("0x0")) {
    throw new Error("QUANTITY must not have leading zero");
  }
  return BigInt(value);
}

export function bytesToQuantity(bytes: Uint8Array): bigint {
  const hex = Buffer.from(bytes).toString("hex").replace(/^0+/, "");
  if (hex.length === 0) {
    return 0n;
  }
  return BigInt(`0x${hex}`);
}

export function ensureLen(bytes: Uint8Array, len: number, label: string): Uint8Array {
  if (bytes.length !== len) {
    throw new Error(`${label} must be ${len} bytes`);
  }
  return bytes;
}
