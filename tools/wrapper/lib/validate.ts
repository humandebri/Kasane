// どこで: BFF入力検証 / 何を: submit/status入力を厳密検証 / なぜ: canister呼び出し前に不正入力を遮断するため

import { Principal } from "@dfinity/principal";
import { z } from "zod";
import type { SubmitPayload } from "./types";
import { parseRequestIdHex } from "./utils";

const principalText = z.string().trim().min(3).max(200).refine((value) => {
  try {
    Principal.fromText(value);
    return true;
  } catch {
    return false;
  }
}, "principal.invalid");

const amountText = z
  .string()
  .trim()
  .regex(/^[0-9]+$/, "amount.decimal_only")
  .refine((value) => {
    try {
      return BigInt(value) > 0n;
    } catch {
      return false;
    }
  }, "amount.invalid");

const submitSchema = z.object({
  assetId: principalText,
  amount: amountText,
  recipient: principalText,
}).strict();

export function parseSubmitPayload(input: unknown): SubmitPayload {
  const parsed = submitSchema.safeParse(input);
  if (!parsed.success) {
    const issue = parsed.error.issues[0];
    const code = issue?.message ?? "invalid_payload";
    throw new Error(`validation.${code}`);
  }
  return parsed.data;
}

export function assertValidRequestIdHex(requestId: string): void {
  parseRequestIdHex(requestId);
}
