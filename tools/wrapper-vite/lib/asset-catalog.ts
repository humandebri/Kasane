// どこで: wrapper asset catalog / 何を: プリセット候補とcustom asset正規化を提供 / なぜ: assetId手入力を減らしつつ、ユーザー追加を安全に永続化するため

import { principalTextToBytes } from "@/lib/principal";

export type AssetOption = {
  assetId: string;
  label: string;
  source: "preset" | "custom" | "token_list";
};

export type CustomAssetDraft = {
  assetId: string;
  label: string;
};

export const CUSTOM_ASSET_STORAGE_KEY = "wrapper.customAssets.v1";
export const DEFAULT_ASSET_ID = "ryjl3-tyaaa-aaaaa-aaaba-cai";
export const LOCAL_TEST_ASSET_ID = "xafvr-biaaa-aaaai-aql5q-cai";
export const MAINNET_IC_HOST = "https://icp-api.io";

const MAINNET_PRESET_ASSETS: AssetOption[] = [
  { assetId: DEFAULT_ASSET_ID, label: "ICP", source: "preset" },
  { assetId: "mxzaz-hqaaa-aaaar-qaada-cai", label: "ckBTC", source: "preset" },
  { assetId: "ss2fx-dyaaa-aaaar-qacoq-cai", label: "ckETH", source: "preset" },
  { assetId: "xevnm-gaaaa-aaaar-qafnq-cai", label: "ckUSDC", source: "preset" },
];

const LOCAL_PRESET_ASSETS: AssetOption[] = [
  { assetId: LOCAL_TEST_ASSET_ID, label: "TESTICP", source: "preset" },
];

type UnknownRecord = { [key: string]: unknown };

function isUnknownRecord(value: unknown): value is UnknownRecord {
  return typeof value === "object" && value !== null;
}

function configuredIcHost(): string | null {
  const host = import.meta.env?.VITE_IC_HOST;
  if (typeof host !== "string") {
    return null;
  }
  const trimmed = host.trim();
  return trimmed === "" ? null : trimmed;
}

function usesLocalIcHost(icHost: string | null): boolean {
  if (icHost === null) {
    return false;
  }
  return icHost.startsWith("http://127.0.0.1:")
    || icHost.startsWith("http://localhost:")
    || icHost.startsWith("https://127.0.0.1:")
    || icHost.startsWith("https://localhost:");
}

export function presetAssetOptions(icHost: string | null = configuredIcHost()): AssetOption[] {
  const presets = usesLocalIcHost(icHost)
    ? [...LOCAL_PRESET_ASSETS, ...MAINNET_PRESET_ASSETS]
    : MAINNET_PRESET_ASSETS;
  return presets.map((asset) => ({ ...asset }));
}

export function resolveLedgerQueryHost(
  assetId: string,
  icHost: string | null = configuredIcHost(),
): string | null {
  if (!usesLocalIcHost(icHost)) {
    return icHost;
  }
  return assetId === LOCAL_TEST_ASSET_ID ? icHost : MAINNET_IC_HOST;
}

export function normalizeCustomAssetDraft(draft: CustomAssetDraft): AssetOption {
  const assetId = draft.assetId.trim();
  const label = draft.label.trim();
  if (label === "") {
    throw new Error("validation.asset_label_required");
  }
  principalTextToBytes(assetId);
  return { assetId, label, source: "custom" };
}

export function parseStoredCustomAssets(text: string | null): AssetOption[] {
  if (text === null || text.trim() === "") {
    return [];
  }
  const parsed: unknown = JSON.parse(text);
  if (!Array.isArray(parsed)) {
    throw new Error("asset_catalog.storage_invalid");
  }
  const out: AssetOption[] = [];
  for (const entry of parsed) {
    if (!isUnknownRecord(entry)) {
      throw new Error("asset_catalog.storage_invalid");
    }
    const assetId = typeof entry.assetId === "string" ? entry.assetId : "";
    const label = typeof entry.label === "string" ? entry.label : "";
    out.push(normalizeCustomAssetDraft({ assetId, label }));
  }
  return dedupeAssetOptions(out);
}

export function serializeCustomAssets(assets: AssetOption[]): string {
  return JSON.stringify(
    assets.map((asset) => ({
      assetId: asset.assetId,
      label: asset.label,
    })),
  );
}

export function dedupeAssetOptions(assets: AssetOption[]): AssetOption[] {
  const seen = new Set<string>();
  const out: AssetOption[] = [];
  for (const asset of assets) {
    if (seen.has(asset.assetId)) {
      continue;
    }
    seen.add(asset.assetId);
    out.push(asset);
  }
  return out;
}

export function mergeAssetOptions(
  customAssets: AssetOption[],
  icHost: string | null = configuredIcHost(),
): AssetOption[] {
  return dedupeAssetOptions([...presetAssetOptions(icHost), ...customAssets]);
}

export const assetCatalogTestHooks = {
  usesLocalIcHost,
  resolveLedgerQueryHost,
};
