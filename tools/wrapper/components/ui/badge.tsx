// どこで: UI共通 / 何を: Badgeコンポーネントを提供 / なぜ: dispatch/executionの意味差を色で区別するため

import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const badgeVariants = cva("inline-flex items-center rounded-md px-2 py-0.5 text-xs font-medium", {
  variants: {
    variant: {
      neutral: "bg-zinc-100 text-zinc-800",
      info: "bg-sky-100 text-sky-800",
      success: "bg-emerald-100 text-emerald-800",
      danger: "bg-rose-100 text-rose-800",
    },
  },
  defaultVariants: {
    variant: "neutral",
  },
});

export function Badge({ className, variant, ...props }: React.ComponentProps<"span"> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}
