// どこで: Juno 設定 / 何を: frontend 配備先と serverless functions の土台を宣言 / なぜ: wrapper-vite を Juno へ段階移行できるようにするため

import { Principal } from "@icp-sdk/core/principal";
import { defineConfig } from "@junobuild/config";

type ConfigEnv = Record<string, string | undefined>;

function optionalEnv(name: string): string | undefined {
  const value = process.env[name];
  if (value === undefined) {
    return undefined;
  }
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

function envOrDefault(name: string, fallback: string): string {
  return optionalEnv(name) ?? fallback;
}

function trimOptionalValue(value: string | undefined): string | null {
  if (value === undefined) {
    return null;
  }
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

function usesLocalIcHost(env: ConfigEnv = process.env): boolean {
  const icHost = trimOptionalValue(env.VITE_IC_HOST);
  if (icHost === null) {
    return false;
  }
  return icHost.startsWith("http://127.0.0.1:")
    || icHost.startsWith("http://localhost:")
    || icHost.startsWith("https://127.0.0.1:")
    || icHost.startsWith("https://localhost:");
}

function resolveBaseAllowedTargets(env: ConfigEnv = process.env): string[] {
  const baseTargets = [
    trimOptionalValue(env.VITE_WRAP_CANISTER_ID) ?? "lpuz5-uyaaa-aaaam-ah4da-cai",
    trimOptionalValue(env.VITE_KASANE_EVM_CANISTER_ID) ?? "4c52m-aiaaa-aaaam-agwwa-cai",
    "ryjl3-tyaaa-aaaaa-aaaba-cai",
    "mxzaz-hqaaa-aaaar-qaada-cai",
    "ss2fx-dyaaa-aaaar-qacoq-cai",
    "xevnm-gaaaa-aaaar-qafnq-cai",
  ];
  if (usesLocalIcHost(env)) {
    return [
      ...baseTargets,
      "xafvr-biaaa-aaaai-aql5q-cai",
    ];
  }
  return baseTargets;
}

function parseConfiguredAllowedTargets(value: string | undefined): string[] {
  const configured = trimOptionalValue(value);
  if (configured === null) {
    return [];
  }

  const out: string[] = [];
  const seen = new Set<string>();
  for (const entry of configured.split(",")) {
    const principalText = entry.trim();
    if (principalText === "" || seen.has(principalText)) {
      continue;
    }
    Principal.fromText(principalText);
    seen.add(principalText);
    out.push(principalText);
  }
  return out;
}

function resolveAllowedTargets(env: ConfigEnv = process.env): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  for (const principalText of [
    ...resolveBaseAllowedTargets(env),
    ...parseConfiguredAllowedTargets(env.JUNO_AUTH_ALLOWED_TARGETS),
  ]) {
    if (seen.has(principalText)) {
      continue;
    }
    seen.add(principalText);
    out.push(principalText);
  }
  return out;
}

export default defineConfig({
  satellite: {
    ids: {
      development: process.env.JUNO_DEV_SATELLITE_ID ?? "REPLACE_WITH_JUNO_DEV_SATELLITE_ID",
      production:
        process.env.JUNO_PROD_SATELLITE_ID
        ?? process.env.JUNO_SATELLITE_ID
        ?? "REPLACE_WITH_JUNO_PROD_SATELLITE_ID",
    },
    source: "dist",
    predeploy: ["npm run build"],
    authentication: {
      internetIdentity: {
        derivationOrigin: optionalEnv("VITE_II_DERIVATION_ORIGIN"),
      },
    },
  },
});

export const junoConfigTestHooks = {
  parseConfiguredAllowedTargets,
  resolveAllowedTargets,
  usesLocalIcHost,
};
