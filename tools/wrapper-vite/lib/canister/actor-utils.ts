// どこで: canister client 共通基盤 / 何を: actor生成とquery/update cacheを薄く共通化 / なぜ: Oisy signer agent と identity の両方を同じ経路で扱うため

import { Actor, type Agent, type Identity } from "@icp-sdk/core/agent";
import { IDL } from "@icp-sdk/core/candid";
import type { Principal } from "@icp-sdk/core/principal";
import { loadConfig } from "../config";
import { getIdentityAgent, getQueryAgent } from "./agent";
import { toAuthenticatedCaller, type AuthenticatedCaller } from "./authenticated-caller";

export async function createQueryActor<TActor>(args: {
  canisterId: string | Principal;
  idlFactory: IDL.InterfaceFactory;
  queryHost?: string;
}): Promise<TActor> {
  return Actor.createActor<TActor>(args.idlFactory, {
    canisterId: args.canisterId,
    agent: await getQueryAgent(args.queryHost === undefined ? undefined : {
      loadConfig: () => ({
        ...loadConfig(),
        icHost: args.queryHost ?? loadConfig().icHost,
      }),
    }),
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

export async function createAgentActor<TActor>(args: {
  canisterId: string | Principal;
  idlFactory: IDL.InterfaceFactory;
  agent: Agent;
}): Promise<TActor> {
  return Actor.createActor<TActor>(args.idlFactory, {
    canisterId: args.canisterId,
    agent: args.agent,
  });
}

export async function createAuthenticatedActor<TActor>(args: {
  canisterId: string | Principal;
  idlFactory: IDL.InterfaceFactory;
  caller: AuthenticatedCaller | Identity;
}): Promise<TActor> {
  const caller = toAuthenticatedCaller(args.caller);
  if ("agent" in caller) {
    return createAgentActor<TActor>({
      canisterId: args.canisterId,
      idlFactory: args.idlFactory,
      agent: caller.agent,
    });
  }
  return createIdentityActor<TActor>({
    canisterId: args.canisterId,
    idlFactory: args.idlFactory,
    identity: caller.identity,
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
    async getSubmitActor(
      callerOrIdentity: AuthenticatedCaller | Identity,
      create: (caller: AuthenticatedCaller) => Promise<TActor>,
    ): Promise<TSubmitActor> {
      if (mockSubmitActor !== null) {
        return mockSubmitActor;
      }
      const caller = toAuthenticatedCaller(callerOrIdentity);
      const key = caller.cacheKey ?? caller.principalText;
      const cached = cachedSubmitActors.get(key);
      if (cached !== undefined) {
        return cached;
      }
      const actor = await create(caller);
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
