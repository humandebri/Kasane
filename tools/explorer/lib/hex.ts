// どこで: 16進ユーティリティ / 何を: URL文字列とblobを相互変換 / なぜ: tx検索の入力/表示を一貫化するため

const HEX_PREFIX = "0x";

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

export function shortHex(value: string, keep: number = 10): string {
  if (value.length <= keep * 2) {
    return value;
  }
  return `${value.slice(0, keep)}...${value.slice(-keep)}`;
}
