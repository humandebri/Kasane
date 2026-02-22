// どこで: verify公開API補助 / 何を: ABI/chainIdの入力整形 / なぜ: Route制約とテスト再利用を両立するため

import { MAX_CHAIN_ID_INT4 } from "./constants";

export function parseVerifiedAbi(abiJson: string): { abi: unknown | null; abiParseError: boolean } {
  try {
    return { abi: JSON.parse(abiJson), abiParseError: false };
  } catch {
    return { abi: null, abiParseError: true };
  }
}

export function parseChainId(value: string | null, fallback: number): number | null {
  const parsed = value ? Number.parseInt(value, 10) : fallback;
  if (!Number.isInteger(parsed) || parsed < 0 || parsed > MAX_CHAIN_ID_INT4) {
    return null;
  }
  return parsed;
}
