// どこで: verify submitスクリプト / 何を: deploy直後に認証付きsubmitを実行 / なぜ: 手動漏れを防ぎCIに組み込みやすくするため

import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { buildVerifyAuthToken } from "../lib/verify/token";

type CliConfig = {
  submitUrl: string;
  payloadFile: string;
  kid: string;
  secret: string;
  sub: string;
  scope: string;
  ttlSec: number;
};

async function main(): Promise<void> {
  const cfg = loadCliConfig(process.env);
  const payloadRaw = await readFile(cfg.payloadFile, "utf8");
  JSON.parse(payloadRaw);

  const token = buildVerifyAuthToken({
    kid: cfg.kid,
    secret: cfg.secret,
    sub: cfg.sub,
    scope: cfg.scope,
    expSec: Math.floor(Date.now() / 1000) + cfg.ttlSec,
    jti: randomUUID(),
  });

  const response = await fetch(cfg.submitUrl, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${token}`,
    },
    body: payloadRaw,
  });

  const bodyText = await response.text();
  if (!response.ok) {
    throw new Error(`verify submit failed: status=${response.status} body=${bodyText}`);
  }
  console.log(bodyText);
}

function loadCliConfig(env: NodeJS.ProcessEnv): CliConfig {
  const submitUrl = env.VERIFY_SUBMIT_URL?.trim() || "http://localhost:3000/api/verify/submit";
  const payloadFile = mustEnv(env, "VERIFY_PAYLOAD_FILE");
  const kid = mustEnv(env, "VERIFY_AUTH_KID");
  const secret = mustEnv(env, "VERIFY_AUTH_SECRET");
  const sub = env.VERIFY_AUTH_SUB?.trim() || "deploy-bot";
  const scope = env.VERIFY_AUTH_SCOPE?.trim() || "verify.submit";
  const ttlSec = parsePositiveInt(env.VERIFY_AUTH_TTL_SEC, 300);
  return { submitUrl, payloadFile, kid, secret, sub, scope, ttlSec };
}

function mustEnv(env: NodeJS.ProcessEnv, key: string): string {
  const value = env[key]?.trim();
  if (!value) {
    throw new Error(`${key} is required`);
  }
  return value;
}

function parsePositiveInt(raw: string | undefined, fallback: number): number {
  if (!raw) {
    return fallback;
  }
  const parsed = Number.parseInt(raw, 10);
  if (!Number.isInteger(parsed) || parsed <= 0) {
    return fallback;
  }
  return parsed;
}

main().catch((err) => {
  console.error(err);
  process.exitCode = 1;
});
