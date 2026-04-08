// どこで: 共通UI / 何を: shadcn準拠のCommandラッパを提供 / なぜ: 検索付きselectorを軽い依存で構成するため

"use client";

import { Command as CommandPrimitive } from "cmdk";
import { Search } from "lucide-react";
import type * as React from "react";
import { cn } from "@/lib/utils";

export function Command({ className, ...props }: React.ComponentProps<typeof CommandPrimitive>) {
  return (
    <CommandPrimitive
      className={cn("flex h-full w-full flex-col overflow-hidden rounded-xl bg-white text-zinc-950", className)}
      {...props}
    />
  );
}

export function CommandInput(
  { className, ...props }: React.ComponentProps<typeof CommandPrimitive.Input>,
) {
  return (
    <div className="flex items-center border-b border-zinc-200 px-3" cmdk-input-wrapper="">
      <Search className="mr-2 size-4 shrink-0 text-zinc-500" />
      <CommandPrimitive.Input
        className={cn(
          "flex h-11 w-full rounded-md bg-transparent py-3 text-sm outline-none placeholder:text-zinc-500 disabled:cursor-not-allowed disabled:opacity-50",
          className,
        )}
        {...props}
      />
    </div>
  );
}

export function CommandList({ className, ...props }: React.ComponentProps<typeof CommandPrimitive.List>) {
  return <CommandPrimitive.List className={cn("max-h-64 overflow-y-auto overflow-x-hidden", className)} {...props} />;
}

export function CommandEmpty(props: React.ComponentProps<typeof CommandPrimitive.Empty>) {
  return <CommandPrimitive.Empty className="py-6 text-center text-sm text-zinc-500" {...props} />;
}

export function CommandGroup({ className, ...props }: React.ComponentProps<typeof CommandPrimitive.Group>) {
  return (
    <CommandPrimitive.Group
      className={cn("overflow-hidden p-2 text-zinc-950 [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-zinc-500", className)}
      {...props}
    />
  );
}

export function CommandItem({ className, ...props }: React.ComponentProps<typeof CommandPrimitive.Item>) {
  return (
    <CommandPrimitive.Item
      className={cn(
        "relative flex cursor-default select-none items-center gap-2 rounded-lg px-2 py-2 text-sm outline-none data-[disabled=true]:pointer-events-none data-[selected=true]:bg-emerald-50 data-[selected=true]:text-emerald-950 data-[disabled=true]:opacity-50",
        className,
      )}
      {...props}
    />
  );
}
