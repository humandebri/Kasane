/// <reference path="./globals.d.ts" />
// どこで: indexer共通ユーティリティ / 何を: 時刻・JSON・I/O補助 / なぜ: ループ本体を単純化するため

import { promises as fs } from "node:fs";

export function nextBackoff(current: number, max: number): number {
  const next = current * 2;
  return next > max ? max : next;
}

export function setupSignalHandlers(chainId: string, onStop: () => void): () => void {
  const handler = (signal: NodeJS.Signals) => {
    process.stderr.write(
      `${JSON.stringify({ ts_ms: Date.now(), level: "info", event: "signal", chain_id: chainId, pid: process.pid, signal })}\n`
    );
    onStop();
  };
  process.on("SIGINT", handler);
  process.on("SIGTERM", handler);
  return () => {
    process.off("SIGINT", handler);
    process.off("SIGTERM", handler);
  };
}

export function setupFatalHandlers(onFatal: (err: unknown) => void): () => void {
  const uncaughtExceptionHandler = (err: unknown) => {
    onFatal(err);
  };
  const unhandledRejectionHandler = (err: unknown) => {
    onFatal(err);
  };
  process.on("uncaughtException", uncaughtExceptionHandler);
  process.on("unhandledRejection", unhandledRejectionHandler);
  return () => {
    process.off("uncaughtException", uncaughtExceptionHandler);
    process.off("unhandledRejection", unhandledRejectionHandler);
  };
}

export function toDayKey(): number {
  const now = new Date();
  const year = now.getUTCFullYear();
  const month = String(now.getUTCMonth() + 1).padStart(2, "0");
  const day = String(now.getUTCDate()).padStart(2, "0");
  return Number(`${year}${month}${day}`);
}

export function jsonStringifyBigInt(value: unknown): string {
  return JSON.stringify(value, (_k, v) => (typeof v === "bigint" ? v.toString() : v));
}

export async function getFileSize(filePath: string): Promise<number> {
  try {
    const stat = await fs.stat(filePath);
    if (!stat.isFile()) {
      return 0;
    }
    return stat.size;
  } catch {
    return 0;
  }
}
