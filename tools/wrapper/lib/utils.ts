// どこで: UIユーティリティ / 何を: className結合関数とhex変換を提供 / なぜ: UI表現とID表現を一貫させるため

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function bytesToHex(bytes: Uint8Array): string {
  return `0x${Buffer.from(bytes).toString("hex")}`;
}

export function hexToBytes(hex: string): Uint8Array {
  const normalized = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (normalized.length % 2 !== 0) {
    throw new Error("hex.length_invalid");
  }
  if (!/^[0-9a-fA-F]*$/.test(normalized)) {
    throw new Error("hex.char_invalid");
  }
  return Uint8Array.from(Buffer.from(normalized, "hex"));
}

export function parseRequestIdHex(text: string): Uint8Array {
  const bytes = hexToBytes(text);
  if (bytes.length !== 32) {
    throw new Error("request_id.length_invalid");
  }
  return bytes;
}
