// どこで: 16進ユーティリティ / 何を: URL文字列とblobを相互変換 / なぜ: tx検索の入力/表示を一貫化するため

const HEX_PREFIX = "0x";
const ADDRESS_HEX_BYTES = 20;
const TX_HASH_HEX_BYTES = 32;

export function toHexLower(bytes: Uint8Array): string {
  return HEX_PREFIX + Buffer.from(bytes).toString("hex");
}

export function parseHex(input: string): Uint8Array {
  const normalized = input.startsWith(HEX_PREFIX) ? input.slice(2) : input;
  if (normalized.length === 0 || normalized.length % 2 !== 0) {
    throw new Error("hex length must be even and non-empty");
  }
  if (!/^[0-9a-fA-F]+$/.test(normalized)) {
    throw new Error("hex must only include 0-9a-fA-F");
  }
  return Uint8Array.from(Buffer.from(normalized, "hex"));
}

export function normalizeHex(input: string): string {
  const trimmed = input.trim().toLowerCase();
  if (trimmed.startsWith(HEX_PREFIX)) {
    return trimmed;
  }
  return `${HEX_PREFIX}${trimmed}`;
}

export function parseAddressHex(input: string): Uint8Array {
  const bytes = parseHex(input);
  if (bytes.length !== ADDRESS_HEX_BYTES) {
    throw new Error("address must be 20 bytes");
  }
  return bytes;
}

export function isAddressHex(input: string): boolean {
  const normalized = normalizeHex(input);
  return /^0x[0-9a-f]{40}$/.test(normalized);
}

export function isTxHashHex(input: string): boolean {
  const normalized = normalizeHex(input);
  return new RegExp(`^0x[0-9a-f]{${TX_HASH_HEX_BYTES * 2}}$`).test(normalized);
}

export function bytesToBigInt(bytes: Uint8Array): bigint {
  let out = 0n;
  for (const value of bytes) {
    out = (out << 8n) | BigInt(value);
  }
  return out;
}

export function shortHex(value: string, keep: number = 10): string {
  if (value.length <= keep * 2) {
    return value;
  }
  return `${value.slice(0, keep)}...${value.slice(-keep)}`;
}
