// どこで: swap panel asset selector / 何を: 検索付きasset selectorとcustom追加を提供 / なぜ: ledger選択を主要導線へ引き上げて、毎回の確認負荷を減らすため

import { Check, ChevronsUpDown, PlusCircle } from "lucide-react";
import { useMemo, useState, type ReactElement } from "react";
import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandGroup,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { AssetOption, CustomAssetDraft } from "@/lib/asset-catalog";
import { cn } from "@/lib/utils";

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
  const [open, setOpen] = useState(false);
  const [customLabel, setCustomLabel] = useState("");
  const [customAssetId, setCustomAssetId] = useState("");
  const [error, setError] = useState<string | null>(null);
  const selected = useMemo(
    () => props.options.find((asset) => asset.assetId === props.value) ?? null,
    [props.options, props.value],
  );

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
      setOpen(false);
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : "validation.asset_id.invalid");
    }
  }

  return (
    <div className="space-y-3 rounded-2xl border border-zinc-200 bg-white p-3 shadow-sm">
      <div className="space-y-1">
        <p className="text-xs font-semibold uppercase tracking-[0.18em] text-zinc-500">Ledger</p>
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <Button
              type="button"
              variant="outline"
              role="combobox"
              aria-expanded={open}
              className="h-12 w-full justify-between rounded-xl border-zinc-300 bg-zinc-50 px-3 text-left"
            >
              <span className="flex min-w-0 flex-col items-start">
                <span className="truncate text-sm font-medium text-zinc-900">
                  {selected ? selected.label : props.selectPlaceholder}
                </span>
                <span className="truncate text-xs text-zinc-500">
                  {selected ? `${selected.assetId} [${selected.source}]` : "ledger principal を選択"}
                </span>
              </span>
              <ChevronsUpDown className="ml-2 size-4 shrink-0 text-zinc-500" />
            </Button>
          </PopoverTrigger>
          <PopoverContent align="start" className="w-[min(92vw,28rem)]">
            <Command>
              <CommandInput placeholder="Search ledger or principal..." />
              <CommandList>
                <CommandEmpty>候補がありません</CommandEmpty>
                <CommandGroup heading="Assets">
                  {props.options.map((asset) => (
                    <CommandItem
                      key={asset.assetId}
                      value={`${asset.label} ${asset.assetId} ${asset.source}`}
                      onSelect={() => {
                        setError(null);
                        props.onChange(asset.assetId);
                        setOpen(false);
                      }}
                    >
                      <Check
                        className={cn(
                          "size-4 text-emerald-600",
                          asset.assetId === props.value ? "opacity-100" : "opacity-0",
                        )}
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-medium">{asset.label}</span>
                        <span className="block truncate text-xs text-zinc-500">
                          {asset.assetId} [{asset.source}]
                        </span>
                      </span>
                    </CommandItem>
                  ))}
                </CommandGroup>
              </CommandList>
            </Command>
          </PopoverContent>
        </Popover>
      </div>
      <div className="grid gap-2 sm:grid-cols-[minmax(0,0.8fr)_minmax(0,1.5fr)_auto]">
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
        <Button type="button" variant="secondary" onClick={handleAdd} className="gap-2">
          <PlusCircle className="size-4" />
          {props.addLabel}
        </Button>
      </div>
      <p className="text-xs text-zinc-600">
        {selected
          ? `selected ledger: ${selected.assetId}`
          : "preset を選ぶか、custom asset を追加してください。"}
      </p>
      {error ? <p className="text-xs text-rose-700">{error}</p> : null}
    </div>
  );
}
