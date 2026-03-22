// どこで: Recent Requests 共通定義 / 何を: 履歴ドキュメントの型・検証・重複排除を提供 / なぜ: frontend と Juno function の契約を1箇所に寄せるため
// 正本: tools/wrapper-vite/lib/recent-requests.ts / wrapper 側は従属コピー。契約変更時は wrapper-vite を先に更新し、このファイルも同期すること。

import { z } from "zod";
import type { HistoryEntry } from "@/components/dashboard-ui/types";

export const RECENT_REQUESTS_LIMIT = 20;

const REQUEST_ID_PATTERN = /^0x[0-9a-f]{64}$/;

export const RecentRequestDocSchema = z.object({
  principalText: z.string().min(1, "history.principal_required"),
  requestId: z.string().regex(REQUEST_ID_PATTERN, "history.request_id_invalid"),
  kind: z.enum(["wrap", "unwrap"]),
  submittedAt: z.string().min(1, "history.submitted_at_required"),
});

export type RecentRequestDoc = z.infer<typeof RecentRequestDocSchema>;

export function createRecentRequestKey(principalText: string, requestId: string): string {
  const normalized = RecentRequestDocSchema.pick({
    principalText: true,
    requestId: true,
  }).parse({ principalText, requestId });
  return `${normalized.principalText}:${normalized.requestId}`;
}

export function toHistoryEntry(doc: RecentRequestDoc): HistoryEntry {
  return {
    requestId: doc.requestId,
    kind: doc.kind,
    submittedAt: doc.submittedAt,
  };
}

export function toRecentRequestDoc(
  principalText: string,
  entry: HistoryEntry,
): RecentRequestDoc {
  return RecentRequestDocSchema.parse({
    principalText,
    requestId: entry.requestId,
    kind: entry.kind,
    submittedAt: entry.submittedAt,
  });
}

export function mergeRecentRequestHistory(
  entries: HistoryEntry[],
  nextEntry: HistoryEntry,
  limit = RECENT_REQUESTS_LIMIT,
): HistoryEntry[] {
  const filtered = entries.filter((entry) => entry.requestId !== nextEntry.requestId);
  return [nextEntry, ...filtered].slice(0, limit);
}
