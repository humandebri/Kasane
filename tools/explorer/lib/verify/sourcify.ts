// どこで: verify補助照合 / 何を: Sourcifyのfull/partial matchを参照 / なぜ: 自前照合の補助診断を付与するため

import { loadConfig } from "../config";

export type SourcifyStatus = "full_match" | "partial_match" | "not_found" | "error";

export async function querySourcifyStatus(chainId: number, contractAddress: string): Promise<SourcifyStatus> {
  const cfg = loadConfig(process.env);
  if (!cfg.verifySourcifyEnabled) {
    return "not_found";
  }
  const normalizedAddress = contractAddress.toLowerCase();
  try {
    const fullMatch = await headWithTimeout(
      `${cfg.verifySourcifyBaseUrl}/contracts/full_match/${chainId}/${normalizedAddress}/metadata.json`
    );
    if (fullMatch) {
      return "full_match";
    }
    const partialMatch = await headWithTimeout(
      `${cfg.verifySourcifyBaseUrl}/contracts/partial_match/${chainId}/${normalizedAddress}/metadata.json`
    );
    if (partialMatch) {
      return "partial_match";
    }
    return "not_found";
  } catch {
    return "error";
  }
}

async function headWithTimeout(url: string): Promise<boolean> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 10_000);
  try {
    const res = await fetch(url, { method: "HEAD", signal: controller.signal, cache: "no-store" });
    return res.ok;
  } finally {
    clearTimeout(timer);
  }
}
