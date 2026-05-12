// どこで: UI共通 / 何を: Inputコンポーネントを提供 / なぜ: フォーム入力体験を統一するため

import * as React from "react";
import { cn } from "@/lib/utils";

export function Input({ className, type = "text", ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      className={cn(
        "flex h-10 w-full rounded-md border border-zinc-300 bg-white px-3 text-sm outline-none transition focus-visible:ring-2 focus-visible:ring-sky-300/60",
        className
      )}
      {...props}
    />
  );
}
