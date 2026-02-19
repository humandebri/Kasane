// どこで: verify source bundle 補助 / 何を: 圧縮JSONの読み書きを共通化 / なぜ: API/UI間の重複実装を削減するため

import { gunzipSync, gzipSync } from "node:zlib";

export function encodeSourceBundle(sourceBundle: Record<string, string>): {
  raw: Uint8Array;
  gzip: Uint8Array;
} {
  const raw = Buffer.from(JSON.stringify(sourceBundle), "utf8");
  return {
    raw,
    gzip: gzipSync(raw),
  };
}

export function decodeSourceBundleFromGzip(blob: Uint8Array): Record<string, string> | null {
  try {
    const json = gunzipSync(Buffer.from(blob)).toString("utf8");
    const parsed = JSON.parse(json);
    if (!isRecord(parsed)) {
      return null;
    }
    const out: Record<string, string> = {};
    for (const [key, value] of Object.entries(parsed)) {
      if (typeof value === "string") {
        out[key] = value;
      }
    }
    return out;
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
