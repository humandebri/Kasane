// どこで: Juno Recent Requests client / 何を: satellite custom functions の query/update を呼ぶ / なぜ: dashboard 履歴の取得と保存を frontend から統一するため
// 正本: tools/wrapper-vite/lib/canister/recent-requests-client.ts / wrapper 側は従属コピー。契約変更時は wrapper-vite を先に更新し、このファイルも同期すること。

import { Actor, type Identity } from "@dfinity/agent";
import { IDL } from "@dfinity/candid";
import { Principal } from "@dfinity/principal";
import { z } from "zod";
import type { HistoryEntry } from "@/components/dashboard-ui/types";
import { getIdentityAgent } from "@/lib/canister/agent";
import { RecentRequestDocSchema, toHistoryEntry, toRecentRequestDoc } from "@/lib/recent-requests";

type RecentRequestsActor = {
  app_list_recent_requests: () => Promise<{
    entriesJson: string;
  }>;
  app_save_recent_request: (args: {
    principalText: string;
    requestId: string;
    kind: string;
    submittedAt: string;
  }) => Promise<{
    principalText: string;
    requestId: string;
    kind: string;
    submittedAt: string;
  }>;
};

const recentRequestsIdlFactory: IDL.InterfaceFactory = ({ IDL: I }) => {
  const RecentRequest = I.Record({
    principalText: I.Text,
    requestId: I.Text,
    kind: I.Text,
    submittedAt: I.Text,
  });
  return I.Service({
    app_list_recent_requests: I.Func([], [I.Record({ entriesJson: I.Text })], ["query"]),
    app_save_recent_request: I.Func([RecentRequest], [RecentRequest], []),
  });
};

const RecentRequestDocsJsonSchema = z.array(RecentRequestDocSchema);

async function createRecentRequestsActor(
  identity: Identity,
  satelliteId: string,
): Promise<RecentRequestsActor> {
  const agent = await getIdentityAgent(identity);
  return Actor.createActor<RecentRequestsActor>(recentRequestsIdlFactory, {
    agent,
    canisterId: Principal.fromText(satelliteId),
  });
}

export async function listRecentRequests(
  identity: Identity,
  principalText: string,
  satelliteId: string,
): Promise<HistoryEntry[]> {
  const actor = await createRecentRequestsActor(identity, satelliteId);
  const docs = await actor.app_list_recent_requests();
  const parsed = RecentRequestDocsJsonSchema.parse(JSON.parse(docs.entriesJson));
  return parsed
    .filter((doc) => doc.principalText === principalText)
    .map(toHistoryEntry);
}

export async function saveRecentRequest(
  identity: Identity,
  principalText: string,
  satelliteId: string,
  entry: HistoryEntry,
): Promise<HistoryEntry> {
  const actor = await createRecentRequestsActor(identity, satelliteId);
  const saved = await actor.app_save_recent_request(
    toRecentRequestDoc(principalText, entry),
  );
  return toHistoryEntry(RecentRequestDocSchema.parse(saved));
}
