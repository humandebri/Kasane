// どこで: canister agent生成 / 何を: query/updateで使うHttpAgentを生成 / なぜ: ブラウザ接続identityで直接update呼び出しするため

import { HttpAgent, type Identity } from "@icp-sdk/core/agent";
import { configTestHooks, loadConfig, type WrapperConfig } from "../config";

const cachedQueryAgents = new Map<string, HttpAgent>();
const cachedIdentityAgents = new Map<string, HttpAgent>();

function resolveBoundFetch(): typeof globalThis.fetch | undefined {
  if (typeof globalThis.fetch !== "function") {
    return undefined;
  }
  return globalThis.fetch.bind(globalThis);
}

async function maybeFetchRootKey(agent: HttpAgent, enabled: boolean): Promise<void> {
  if (enabled) {
    await agent.fetchRootKey();
  }
}

type AgentDeps = {
  loadConfig: () => WrapperConfig;
};

const defaultAgentDeps: AgentDeps = {
  loadConfig,
};

export async function getQueryAgent(deps: AgentDeps = defaultAgentDeps): Promise<HttpAgent> {
  const cfg = deps.loadConfig();
  const cached = cachedQueryAgents.get(cfg.icHost);
  if (cached) {
    return cached;
  }
  const agent = new HttpAgent({ host: cfg.icHost, fetch: resolveBoundFetch() });
  await maybeFetchRootKey(agent, configTestHooks.shouldFetchRootKey(cfg.icHost));
  cachedQueryAgents.set(cfg.icHost, agent);
  return agent;
}

export async function getIdentityAgent(identity: Identity, deps: AgentDeps = defaultAgentDeps): Promise<HttpAgent> {
  const key = identity.getPrincipal().toText();
  const cached = cachedIdentityAgents.get(key);
  if (cached) {
    return cached;
  }
  const cfg = deps.loadConfig();
  const agent = new HttpAgent({
    host: cfg.icHost,
    fetch: resolveBoundFetch(),
    identity,
  });
  await maybeFetchRootKey(agent, configTestHooks.shouldFetchRootKey(cfg.icHost));
  cachedIdentityAgents.set(key, agent);
  return agent;
}

export function resetAgentCache(): void {
  cachedQueryAgents.clear();
  cachedIdentityAgents.clear();
}
