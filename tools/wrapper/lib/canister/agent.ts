// どこで: canister agent生成 / 何を: HttpAgentの共通生成を提供 / なぜ: BFF内の接続設定を一元化するため

import { HttpAgent } from "@dfinity/agent";
import { loadConfig } from "../config";
import { submitIdentityFromSecretHex } from "../identity";

let cachedQueryAgent: HttpAgent | null = null;
let cachedSubmitAgent: HttpAgent | null = null;

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

export async function getSubmitAgent(): Promise<HttpAgent> {
  if (cachedSubmitAgent) {
    return cachedSubmitAgent;
  }
  const cfg = loadConfig();
  if (cfg.submitIdentitySecretKeyHex === null) {
    throw new Error("config.missing:ICP_IDENTITY_SECRET_KEY_HEX");
  }
  const identity = submitIdentityFromSecretHex(cfg.submitIdentitySecretKeyHex);
  const agent = new HttpAgent({ host: cfg.icHost, fetch: globalThis.fetch, identity });
  await maybeFetchRootKey(agent, cfg.fetchRootKey);
  cachedSubmitAgent = agent;
  return agent;
}
