// どこで: UIユーティリティ / 何を: className結合関数を提供 / なぜ: shadcn系コンポーネントの再利用を統一するため

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
