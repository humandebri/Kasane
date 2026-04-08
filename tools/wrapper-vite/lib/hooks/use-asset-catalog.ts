// どこで: wrapper asset catalog hook / 何を: preset + localStorage custom assetを統合 / なぜ: selector候補を共有し、セッションを跨いで保持するため

import { useEffect, useMemo, useState } from "react";
import {
  type AssetOption,
  type CustomAssetDraft,
  CUSTOM_ASSET_STORAGE_KEY,
  mergeAssetOptions,
  normalizeCustomAssetDraft,
  parseStoredCustomAssets,
  serializeCustomAssets,
} from "@/lib/asset-catalog";

export function useAssetCatalog() {
  const [customAssets, setCustomAssets] = useState<AssetOption[]>([]);
  const [storageReady, setStorageReady] = useState(false);

  useEffect(() => {
    try {
      setCustomAssets(
        parseStoredCustomAssets(window.localStorage.getItem(CUSTOM_ASSET_STORAGE_KEY)),
      );
    } catch {
      setCustomAssets([]);
    }
    setStorageReady(true);
  }, []);

  useEffect(() => {
    if (!storageReady) {
      return;
    }
    window.localStorage.setItem(
      CUSTOM_ASSET_STORAGE_KEY,
      serializeCustomAssets(customAssets),
    );
  }, [customAssets, storageReady]);

  const assetOptions = useMemo(() => mergeAssetOptions(customAssets), [customAssets]);

  function addCustomAsset(draft: CustomAssetDraft): AssetOption {
    const normalized = normalizeCustomAssetDraft(draft);
    setCustomAssets((current) => {
      for (const asset of current) {
        if (asset.assetId === normalized.assetId) {
          return current;
        }
      }
      return [...current, normalized];
    });
    return normalized;
  }

  return { assetOptions, addCustomAsset };
}
