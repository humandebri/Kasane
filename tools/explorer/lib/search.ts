// どこで: 検索判定ヘルパー / 何を: 入力文字列から遷移先ルートを決定 / なぜ: ページ実装とテストで同じ判定を使うため

import { isAddressHex, isTxHashHex, normalizeHex } from "./hex";
import { Principal } from "@dfinity/principal";

export function resolveSearchRoute(input: string): string {
  const value = input.trim();
  if (/^[0-9]+$/.test(value)) {
    return `/blocks/${value}`;
  }
  if (isTxHashHex(value)) {
    return `/tx/${normalizeHex(value)}`;
  }
  if (isAddressHex(value)) {
    return `/address/${normalizeHex(value)}`;
  }
  if (isPrincipalText(value)) {
    return `/principal/${encodeURIComponent(value)}`;
  }
  return "/";
}

function isPrincipalText(value: string): boolean {
  try {
    Principal.fromText(value);
    return true;
  } catch {
    return false;
  }
}
