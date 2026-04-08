// どこで: health 共通ロジック / 何を: canister 疎通と設定値の簡易確認を返す / なぜ: Juno Function と手動検証で同じ契約を再利用するため

import { getDispatchResult } from "@/lib/canister/wrapper-client";
import { getExecutionResult } from "@/lib/canister/wrap-client";
import { loadConfig, type WrapperConfig } from "@/lib/config";
import { toApiError, toErrorBody } from "@/lib/errors";
import type { ApiErrorBody, HealthResponse } from "@/lib/types";

export type HealthFunctionResponse = HealthResponse | ApiErrorBody;

export async function probeHealth(
  deps: {
    loadConfig?: () => WrapperConfig;
    getDispatchResult?: typeof getDispatchResult;
    getExecutionResult?: typeof getExecutionResult;
  } = {},
): Promise<HealthFunctionResponse> {
  try {
    const cfg = (deps.loadConfig ?? loadConfig)();
    const probe = new Uint8Array(32);
    const [gatewayProbe, wrapProbe] = await Promise.allSettled([
      (deps.getDispatchResult ?? getDispatchResult)(probe),
      (deps.getExecutionResult ?? getExecutionResult)(probe),
    ]);

    return {
      ok: gatewayProbe.status === "fulfilled" && wrapProbe.status === "fulfilled",
      kasaneEvmReachable: gatewayProbe.status === "fulfilled",
      wrapReachable: wrapProbe.status === "fulfilled",
      icHost: cfg.icHost,
      kasaneEvmCanisterId: cfg.kasaneEvmCanisterId,
      wrapCanisterId: cfg.wrapCanisterId,
    };
  } catch (error) {
    return toErrorBody(toApiError(error, "health_failed"));
  }
}
