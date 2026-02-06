/// <reference path="./globals.d.ts" />
// どこで: indexerエラー分類 / 何を: exportエラーの分類とメトリクス更新 / なぜ: 失敗時の記録を一箇所に集約するため

import { IndexerDb } from "./db";
import { ExportError } from "./types";
import { toDayKey } from "./worker_utils";

export function classifyExportError(
  error: ExportError,
  db: IndexerDb
): { kind: "Pruned" | "InvalidCursor" | "Decode" | "ArchiveIO"; message: string } {
  if ("Pruned" in error) {
    db.setMeta("last_error", "Pruned");
    db.addMetrics(toDayKey(), 0, 0, 0, 1);
    return { kind: "Pruned", message: `Pruned before ${error.Pruned.pruned_before_block.toString()}` };
  }
  if ("InvalidCursor" in error) {
    db.setMeta("last_error", "InvalidCursor");
    db.addMetrics(toDayKey(), 0, 0, 0, 1);
    return { kind: "InvalidCursor", message: `InvalidCursor: ${error.InvalidCursor.message}` };
  }
  if ("MissingData" in error) {
    db.setMeta("last_error", "MissingData");
    db.addMetrics(toDayKey(), 0, 0, 0, 1);
    return { kind: "Decode", message: `MissingData: ${error.MissingData.message}` };
  }
  db.setMeta("last_error", "Limit");
  db.addMetrics(toDayKey(), 0, 0, 0, 1);
  return { kind: "InvalidCursor", message: "Limit: max_bytes invalid" };
}
