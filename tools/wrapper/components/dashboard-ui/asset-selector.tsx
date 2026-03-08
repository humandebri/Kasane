// どこで: swap panel asset selector / 何を: asset候補選択とcustom追加を提供 / なぜ: principal直入力を減らし、よく使うトークンを再利用しやすくするため

import { useState, type ReactElement } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { AssetOption, CustomAssetDraft } from "@/lib/asset-catalog";

export function AssetSelector(props: {
  value: string;
  options: AssetOption[];
  addLabel: string;
  selectPlaceholder: string;
  customLabelPlaceholder: string;
  customAssetPlaceholder: string;
  onChange: (assetId: string) => void;
  onAddCustomAsset: (draft: CustomAssetDraft) => AssetOption;
}): ReactElement {
  const [customLabel, setCustomLabel] = useState("");
  const [customAssetId, setCustomAssetId] = useState("");
  const [error, setError] = useState<string | null>(null);

  function handleAdd(): void {
    try {
      const added = props.onAddCustomAsset({
        label: customLabel,
        assetId: customAssetId,
      });
      setCustomLabel("");
      setCustomAssetId("");
      setError(null);
      props.onChange(added.assetId);
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : "validation.asset_id.invalid");
    }
  }

  return (
    <div className="space-y-2 rounded-xl border border-zinc-200 bg-zinc-50/70 p-3 sm:col-span-2">
      <select
        className="flex h-10 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm outline-none transition focus-visible:ring-2 focus-visible:ring-emerald-400/60"
        value={props.value}
        onChange={(event) => {
          setError(null);
          props.onChange(event.target.value);
        }}
      >
        <option value="">{props.selectPlaceholder}</option>
        {props.options.map((asset) => (
          <option key={asset.assetId} value={asset.assetId}>
            {asset.label} [{asset.source === "preset" ? "preset" : "custom"}]
          </option>
        ))}
      </select>
      <div className="grid gap-2 sm:grid-cols-[1fr_1.4fr_auto]">
        <Input
          placeholder={props.customLabelPlaceholder}
          value={customLabel}
          onChange={(event) => setCustomLabel(event.target.value)}
        />
        <Input
          placeholder={props.customAssetPlaceholder}
          value={customAssetId}
          onChange={(event) => setCustomAssetId(event.target.value)}
        />
        <Button type="button" variant="secondary" onClick={handleAdd}>
          {props.addLabel}
        </Button>
      </div>
      <p className="text-xs text-zinc-600">
        選択値: {props.value === "" ? "(未選択)" : props.value}
      </p>
      {error ? <p className="text-xs text-rose-700">{error}</p> : null}
    </div>
  );
}
