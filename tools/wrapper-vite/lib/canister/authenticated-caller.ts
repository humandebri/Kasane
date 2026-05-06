// どこで: canister client 共通型
// 何を: identity または signer agent を使う認証済み caller を表現
// なぜ: Oisy signer と従来 identity の両方で update actor を生成するため

import type { Agent, Identity } from "@icp-sdk/core/agent";

export type AuthenticatedCaller =
  | {
    principalText: string;
    cacheKey?: string;
    agent: Agent;
  }
  | {
    principalText: string;
    cacheKey?: string;
    identity: Identity;
  };

export function toAuthenticatedCaller(
  value: AuthenticatedCaller | Identity,
): AuthenticatedCaller {
  if ("principalText" in value) {
    return value;
  }
  return {
    principalText: value.getPrincipal().toText(),
    identity: value,
  };
}
