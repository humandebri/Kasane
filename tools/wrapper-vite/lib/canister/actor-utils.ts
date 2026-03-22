// どこで: canister client 共通基盤 / 何を: actor生成とquery/update cacheを薄く共通化 / なぜ: clientごとの初期化重複を減らし、変更漏れを防ぐため

import { Actor, type Identity } from "@icp-sdk/core/agent";
import { IDL } from "@icp-sdk/core/candid";
import type { Principal } from "@icp-sdk/core/principal";
import { getIdentityAgent, getQueryAgent } from "./agent";

export async function createQueryActor<TActor>(args: {
  canisterId: string | Principal;
  idlFactory: IDL.InterfaceFactory;
}): Promise<TActor> {
  return Actor.createActor<TActor>(args.idlFactory, {
    canisterId: args.canisterId,
    agent: await getQueryAgent(),
  });
}

export async function createIdentityActor<TActor>(args: {
  canisterId: string | Principal;
  idlFactory: IDL.InterfaceFactory;
  identity: Identity;
}): Promise<TActor> {
  return Actor.createActor<TActor>(args.idlFactory, {
    canisterId: args.canisterId,
    agent: await getIdentityAgent(args.identity),
  });
}

export function createActorCache<
  TQueryActor,
  TSubmitActor,
  TActor extends TQueryActor & TSubmitActor,
>() {
  let cachedQueryActor: TActor | null = null;
  const cachedSubmitActors = new Map<string, TActor>();
  let mockQueryActor: TQueryActor | null = null;
  let mockSubmitActor: TSubmitActor | null = null;

  return {
    async getQueryActor(create: () => Promise<TActor>): Promise<TQueryActor> {
      if (mockQueryActor !== null) {
        return mockQueryActor;
      }
      if (cachedQueryActor !== null) {
        return cachedQueryActor;
      }
      cachedQueryActor = await create();
      return cachedQueryActor;
    },
    async getSubmitActor(identity: Identity, create: (identity: Identity) => Promise<TActor>): Promise<TSubmitActor> {
      if (mockSubmitActor !== null) {
        return mockSubmitActor;
      }
      const key = identity.getPrincipal().toText();
      const cached = cachedSubmitActors.get(key);
      if (cached !== undefined) {
        return cached;
      }
      const actor = await create(identity);
      cachedSubmitActors.set(key, actor);
      return actor;
    },
    reset(): void {
      cachedQueryActor = null;
      cachedSubmitActors.clear();
      mockQueryActor = null;
      mockSubmitActor = null;
    },
    setMockQueryActor(actor: TQueryActor | null): void {
      mockQueryActor = actor;
    },
    setMockSubmitActor(actor: TSubmitActor | null): void {
      mockSubmitActor = actor;
    },
  };
}
