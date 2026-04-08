// どこで: 共通UI / 何を: shadcn準拠のPopoverラッパを提供 / なぜ: selector系UIを既存スタイルで統一するため

"use client";

import * as PopoverPrimitive from "@radix-ui/react-popover";
import type * as React from "react";
import { cn } from "@/lib/utils";

export function Popover(props: React.ComponentProps<typeof PopoverPrimitive.Root>) {
  return <PopoverPrimitive.Root {...props} />;
}

export function PopoverTrigger(props: React.ComponentProps<typeof PopoverPrimitive.Trigger>) {
  return <PopoverPrimitive.Trigger {...props} />;
}

export function PopoverContent(
  { className, align = "center", sideOffset = 8, ...props }: React.ComponentProps<typeof PopoverPrimitive.Content>,
) {
  return (
    <PopoverPrimitive.Portal>
      <PopoverPrimitive.Content
        align={align}
        sideOffset={sideOffset}
        className={cn(
          "z-50 w-80 rounded-xl border border-zinc-200 bg-white p-0 shadow-lg outline-none",
          className,
        )}
        {...props}
      />
    </PopoverPrimitive.Portal>
  );
}
