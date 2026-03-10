// どこで: UI共通 / 何を: Buttonコンポーネントを提供 / なぜ: 操作ボタンの見た目と状態管理を統一するため

import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 rounded-md text-sm font-medium transition-colors disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default: "bg-zinc-900 text-white hover:bg-zinc-800",
        secondary: "bg-zinc-100 text-zinc-900 hover:bg-zinc-200",
        outline: "border border-zinc-300 bg-white hover:bg-zinc-50",
      },
      size: {
        default: "h-10 px-4 py-2",
        sm: "h-8 rounded-md px-3",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
);

export function Button({ className, variant, size, ...props }: React.ComponentProps<"button"> & VariantProps<typeof buttonVariants>) {
  return <button className={cn(buttonVariants({ variant, size, className }))} {...props} />;
}
