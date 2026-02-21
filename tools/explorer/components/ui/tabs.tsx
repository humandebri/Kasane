// どこで: UI共通 / 何を: Tabsコンポーネントを提供 / なぜ: Data表示切替の操作UIを統一するため

"use client";

import * as React from "react";
import * as TabsPrimitive from "@radix-ui/react-tabs";
import { cn } from "../../lib/utils";

function Tabs({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Root>) {
  return <TabsPrimitive.Root className={cn("inline-flex", className)} {...props} />;
}

function TabsList({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.List>) {
  return (
    <TabsPrimitive.List
      className={cn("inline-flex h-6 items-stretch rounded-md border border-slate-300 bg-white p-0 text-xs text-slate-700", className)}
      {...props}
    />
  );
}

function TabsTrigger({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Trigger>) {
  return (
    <TabsPrimitive.Trigger
      className={cn(
        "inline-flex h-full items-center justify-center px-2 py-0 text-xs leading-none font-medium transition-colors focus-visible:outline-none data-[state=active]:bg-slate-200 data-[state=active]:text-slate-900",
        "first:rounded-l-[5px] last:rounded-r-[5px] [&:not(:first-child)]:border-l [&:not(:first-child)]:border-slate-300",
        className
      )}
      {...props}
    />
  );
}

function TabsContent({ className, ...props }: React.ComponentProps<typeof TabsPrimitive.Content>) {
  return <TabsPrimitive.Content className={cn("mt-0", className)} {...props} />;
}

export { Tabs, TabsList, TabsTrigger, TabsContent };
