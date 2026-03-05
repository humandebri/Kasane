// どこで: canister agent生成 / 何を: query/updateで使うHttpAgentを生成 / なぜ: ブラウザ接続identityで直接update呼び出しするため

import { HttpAgent, type Identity } from "@dfinity/agent";
import { loadConfig } from "../config";

let cachedQueryAgent: HttpAgent | null = null;
const cachedIdentityAgents = new Map<string, HttpAgent>();

async function maybeFetchRootKey(agent: HttpAgent, enabled: boolean): Promise<void> {
  if (enabled) {
    await agent.fetchRootKey();
  }
}

export async function getQueryAgent(): Promise<HttpAgent> {
  if (cachedQueryAgent) {
    return cachedQueryAgent;
  }
  const cfg = loadConfig();
  const agent = new HttpAgent({ host: cfg.icHost, fetch: globalThis.fetch });
  await maybeFetchRootKey(agent, cfg.fetchRootKey);
  cachedQueryAgent = agent;
  return agent;
}

export async function getIdentityAgent(identity: Identity): Promise<HttpAgent> {
  const key = identity.getPrincipal().toText();
  const cached = cachedIdentityAgents.get(key);
  if (cached) {
    return cached;
  }
  const cfg = loadConfig();
  const agent = new HttpAgent({ host: cfg.icHost, fetch: globalThis.fetch, identity });
  await maybeFetchRootKey(agent, cfg.fetchRootKey);
  cachedIdentityAgents.set(key, agent);
  return agent;
}

export function resetAgentCache(): void {
  cachedQueryAgent = null;
  cachedIdentityAgents.clear();
}
