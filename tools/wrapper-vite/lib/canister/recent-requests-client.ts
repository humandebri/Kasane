// どこで: Juno Recent Requests client / 何を: satellite custom functions を query/update する / なぜ: 履歴取得と保存を frontend から統一的に扱うため

import type { Identity } from "@icp-sdk/core/agent";
import { IDL } from "@icp-sdk/core/candid";
import { Principal } from "@icp-sdk/core/principal";
import { z } from "zod";
import { createIdentityActor } from "@/lib/canister/actor-utils";
import type { HistoryEntry } from "@/components/dashboard-ui/types";
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
const cachedRecentRequestActors = new Map<string, Promise<RecentRequestsActor>>();

type RecentRequestsClientDeps = {
  createIdentityActor: typeof createIdentityActor;
};

const defaultRecentRequestsClientDeps: RecentRequestsClientDeps = {
  createIdentityActor,
};

let recentRequestsClientDeps: RecentRequestsClientDeps = defaultRecentRequestsClientDeps;

function createRecentRequestsActorCacheKey(identity: Identity, satelliteId: string): string {
  return `${identity.getPrincipal().toText()}:${satelliteId}`;
}

async function createRecentRequestsActor(
  identity: Identity,
  satelliteId: string,
): Promise<RecentRequestsActor> {
  const key = createRecentRequestsActorCacheKey(identity, satelliteId);
  const cached = cachedRecentRequestActors.get(key);
  if (cached !== undefined) {
    return cached;
  }
  const actorPromise = recentRequestsClientDeps.createIdentityActor<RecentRequestsActor>({
    canisterId: Principal.fromText(satelliteId),
    idlFactory: recentRequestsIdlFactory,
    identity,
  }).catch((error: unknown) => {
    cachedRecentRequestActors.delete(key);
    throw error;
  });
  cachedRecentRequestActors.set(key, actorPromise);
  return actorPromise;
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

export const recentRequestsClientTestHooks = {
  reset(): void {
    cachedRecentRequestActors.clear();
    recentRequestsClientDeps = defaultRecentRequestsClientDeps;
  },
  setDeps(deps: Partial<RecentRequestsClientDeps>): void {
    recentRequestsClientDeps = {
      ...defaultRecentRequestsClientDeps,
      ...deps,
    };
  },
  createRecentRequestsActorCacheKey,
};
