// どこで: UI/canister共通ユーティリティ / 何を: className結合とhex変換を提供 / なぜ: ブラウザ/Node両対応で表現とID処理を一貫化するため

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

export function bytesToHex(bytes: Uint8Array): string {
  let out = "0x";
  for (let i = 0; i < bytes.length; i += 1) {
    const byte = bytes[i];
    if (byte === undefined) {
      throw new Error("hex.byte_missing");
    }
    out += byte.toString(16).padStart(2, "0");
  }
  return out;
}

export function hexToBytes(hex: string): Uint8Array {
  const normalized = hex.startsWith("0x") ? hex.slice(2) : hex;
  if (normalized.length % 2 !== 0) {
    throw new Error("hex.length_invalid");
  }
  if (!/^[0-9a-fA-F]*$/.test(normalized)) {
    throw new Error("hex.char_invalid");
  }
  const out = new Uint8Array(normalized.length / 2);
  for (let i = 0; i < out.length; i += 1) {
    const hi = normalized[i * 2] ?? "0";
    const lo = normalized[i * 2 + 1] ?? "0";
    out[i] = Number.parseInt(`${hi}${lo}`, 16);
  }
  return out;
}

export function parseRequestIdHex(text: string): Uint8Array {
  const bytes = hexToBytes(text);
  if (bytes.length !== 32) {
    throw new Error("request_id.length_invalid");
  }
  return bytes;
}
