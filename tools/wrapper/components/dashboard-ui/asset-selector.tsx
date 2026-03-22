// どこで: swap panel asset selector / 何を: 検索付きasset selectorを提供 / なぜ: asset選択を主要導線へ集約するため

import { Check, ChevronsUpDown } from "lucide-react";
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
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import type { AssetOption } from "@/lib/asset-catalog";
import { cn } from "@/lib/utils";

export function AssetSelector(props: {
  value: string;
  options: AssetOption[];
  selectPlaceholder: string;
  onChange: (assetId: string) => void;
}): ReactElement {
  const [open, setOpen] = useState(false);
  const selected = useMemo(
    () => props.options.find((asset) => asset.assetId === props.value) ?? null,
    [props.options, props.value],
  );

  return (
    <div className="rounded-2xl border border-zinc-200 bg-white p-3 shadow-sm">
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
                {selected ? `${selected.assetId} [${selected.source}]` : "asset を選択"}
              </span>
            </span>
            <ChevronsUpDown className="ml-2 size-4 shrink-0 text-zinc-500" />
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-[min(92vw,28rem)]">
          <Command>
            <CommandInput placeholder="Search asset or principal..." />
            <CommandList>
              <CommandEmpty>候補がありません</CommandEmpty>
              <CommandGroup heading="Assets">
                {props.options.map((asset) => (
                  <CommandItem
                    key={asset.assetId}
                    value={`${asset.label} ${asset.assetId} ${asset.source}`}
                    onSelect={() => {
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
  );
}
