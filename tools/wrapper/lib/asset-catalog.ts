// どこで: wrapper asset catalog / 何を: プリセット候補とcustom asset正規化を提供 / なぜ: assetId手入力を減らしつつ、ユーザー追加を安全に永続化するため

import { principalTextToBytes } from "@/lib/principal";

export type AssetOption = {
  assetId: string;
  label: string;
  source: "preset" | "custom";
};

export type CustomAssetDraft = {
  assetId: string;
  label: string;
};

export const CUSTOM_ASSET_STORAGE_KEY = "wrapper.customAssets.v1";

const PRESET_ASSETS: AssetOption[] = [
  { assetId: "ryjl3-tyaaa-aaaaa-aaaba-cai", label: "ICP", source: "preset" },
  { assetId: "mxzaz-hqaaa-aaaar-qaada-cai", label: "ckBTC", source: "preset" },
  { assetId: "ss2fx-dyaaa-aaaar-qacoq-cai", label: "ckETH", source: "preset" },
  { assetId: "xevnm-gaaaa-aaaar-qafnq-cai", label: "ckUSDC", source: "preset" },
];

type UnknownRecord = { [key: string]: unknown };

function isUnknownRecord(value: unknown): value is UnknownRecord {
  return typeof value === "object" && value !== null;
}

export function presetAssetOptions(): AssetOption[] {
  return PRESET_ASSETS.map((asset) => ({ ...asset }));
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

export function mergeAssetOptions(customAssets: AssetOption[]): AssetOption[] {
  return dedupeAssetOptions([...presetAssetOptions(), ...customAssets]);
}
