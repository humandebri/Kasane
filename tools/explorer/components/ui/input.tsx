// どこで: UI共通 / 何を: Inputコンポーネントを提供 / なぜ: フォームの見た目と操作感を統一するため

import * as React from "react";
import { cn } from "../../lib/utils";

function Input({ className, type = "text", ...props }: React.ComponentProps<"input">) {
  return (
    <input
      type={type}
      className={cn(
        "flex h-10 w-full rounded-full border border-zinc-200 bg-white/90 px-4 text-sm outline-none transition focus-visible:ring-2 focus-visible:ring-sky-400/60",
        className
      )}
      {...props}
    />
  );
}

export { Input };
