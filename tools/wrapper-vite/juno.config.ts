// どこで: Juno 設定 / 何を: frontend 配備先と serverless functions の土台を宣言 / なぜ: wrapper-vite を Juno へ段階移行できるようにするため

import { defineConfig } from "@junobuild/config";

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

function resolveAllowedTargets(): string[] {
  return [
    envOrDefault("VITE_WRAP_CANISTER_ID", "lpuz5-uyaaa-aaaam-ah4da-cai"),
    envOrDefault("VITE_KASANE_EVM_CANISTER_ID", "4c52m-aiaaa-aaaam-agwwa-cai"),
    "xafvr-biaaa-aaaai-aql5q-cai",
    "ryjl3-tyaaa-aaaaa-aaaba-cai",
    "mxzaz-hqaaa-aaaar-qaada-cai",
    "ss2fx-dyaaa-aaaar-qacoq-cai",
    "xevnm-gaaaa-aaaar-qafnq-cai",
  ];
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
      google: {
        clientId:
          optionalEnv("GOOGLE_CLIENT_ID")
          ?? optionalEnv("VITE_GOOGLE_CLIENT_ID")
          ?? "REPLACE_WITH_GOOGLE_CLIENT_ID",
        delegation: {
          allowedTargets: resolveAllowedTargets(),
          sessionDuration: 24n * 60n * 60n * 1_000_000_000n,
        },
      },
    },
  },
});
