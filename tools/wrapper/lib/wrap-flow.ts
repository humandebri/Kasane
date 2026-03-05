// どこで: wrap UIロジック共通 / 何を: fee計算・allowance判定・status終端判定を提供 / なぜ: 画面実装とテストで同一仕様を再利用するため

import type { StatusResponse } from "./types";

export function ceilMulRatio(
  value: bigint,
  numerator: bigint,
  denominator: bigint,
): bigint {
  return (value * numerator + denominator - 1n) / denominator;
}

export function weiToE8sCeil(wei: bigint): bigint {
  return (wei + 10_000_000_000n - 1n) / 10_000_000_000n;
}

export function formatE8sToIcpText(e8s: bigint): string {
  const integer = e8s / 100_000_000n;
  const fraction = (e8s % 100_000_000n).toString().padStart(8, "0");
  return `${integer.toString()}.${fraction}`;
}

export function computeWrapFeeQuote(args: {
  gasPriceWei: bigint;
  gasLimit: bigint;
  cycleFeeE8s: bigint;
  gasPriceBufferBps: bigint;
}): { chargedGasPriceWei: bigint; totalFeeE8s: bigint } {
  const chargedGasPriceWei = ceilMulRatio(
    args.gasPriceWei,
    args.gasPriceBufferBps,
    10_000n,
  );
  const gasFeeE8s = weiToE8sCeil(chargedGasPriceWei * args.gasLimit);
  return {
    chargedGasPriceWei,
    totalFeeE8s: gasFeeE8s + args.cycleFeeE8s,
  };
}

export function computeRequiredAllowances(args: {
  assetLedgerCanister: string;
  feeLedgerCanister: string;
  amount: bigint;
  totalFeeE8s: bigint;
}): { requiredAssetAllowance: bigint; requiredFeeAllowance: bigint } {
  if (args.assetLedgerCanister === args.feeLedgerCanister) {
    return {
      requiredAssetAllowance: args.amount + args.totalFeeE8s,
      requiredFeeAllowance: 0n,
    };
  }
  return {
    requiredAssetAllowance: args.amount,
    requiredFeeAllowance: args.totalFeeE8s,
  };
}

export type StatusPhase =
  | "idle"
  | "submitted"
  | "dispatching"
  | "executing"
  | "done"
  | "failed";

export function deriveStatusPhase(status: StatusResponse | null): StatusPhase {
  if (!status) {
    return "idle";
  }
  if (
    status.dispatchStatus === "DispatchFailed" ||
    status.executionStatus === "Failed"
  ) {
    return "failed";
  }
  if (status.executionStatus === "Succeeded") {
    return "done";
  }
  if (status.executionStatus === "Running") {
    return "executing";
  }
  if (
    status.dispatchStatus === "Dispatched" ||
    status.dispatchStatus === "Dispatching"
  ) {
    return "dispatching";
  }
  return "submitted";
}

export function isTerminalStatus(status: StatusResponse | null): boolean {
  const phase = deriveStatusPhase(status);
  return phase === "done" || phase === "failed";
}
