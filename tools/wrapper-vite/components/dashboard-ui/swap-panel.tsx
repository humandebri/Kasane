// どこで: dashboard swap panel / 何を: Amount中心入力とAdvanced入力を提供 / なぜ: 通常操作を簡略化しつつ運用時の調整余地を残すため

import { ArrowDownUp, Sparkles } from "lucide-react";
import type { ReactElement } from "react";
import { AssetSelector } from "@/components/dashboard-ui/asset-selector";
import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import type { AssetOption } from "@/lib/asset-catalog";
import { formatWeiToGwei2 } from "@/lib/wrap-flow";
import type {
  ActiveTab,
  UnwrapFormState,
  WrapActionStep,
  WrapGasEstimateStatus,
  WrapNonceStatus,
  WrapFormState,
} from "./types";

function wrapStepLabel(step: WrapActionStep): string {
  if (step === "quoting") return "Fee見積を取得中...";
  if (step === "checking_allowance") return "allowanceを確認中...";
  if (step === "approving_asset") return "asset approveを実行中...";
  if (step === "approving_fee") return "fee approveを実行中...";
  if (step === "submitting") return "submit_wrap_request を送信中...";
  if (step === "done") return "送信完了";
  if (step === "error") return "送信失敗";
  return "待機中";
}

export function SwapPanel(props: {
  tab: ActiveTab;
  unwrapForm: UnwrapFormState;
  wrapForm: WrapFormState;
  wrapActionStep: WrapActionStep;
  wrapGasEstimateStatus: WrapGasEstimateStatus;
  wrapGasEstimateError: string | null;
  wrapNonceStatus: WrapNonceStatus;
  wrapNonceError: string | null;
  wrapFeeEstimateText: string | null;
  wrapPreviewRequestId: string | null;
  lastSubmittedWrapRequestId: string | null;
  wrapBalanceText: string | null;
  wrapMaxAmountText: string | null;
  wrapChargedGasPriceWei: string | null;
  wrapMaxPriorityFeePerGasWei: string | null;
  submitLoading: boolean;
  walletConnected: boolean;
  configError: string | null;
  assetOptions: AssetOption[];
  onTabChange: (tab: ActiveTab) => void;
  onUnwrapChange: (next: UnwrapFormState) => void;
  onWrapChange: (next: WrapFormState) => void;
  onSubmitUnwrap: () => void;
  onSubmitWrap: () => void;
}): ReactElement {
  return (
    <Card className="h-full min-w-0 rounded-2xl border-emerald-100">
      <CardHeader>
        <CardTitle className="text-lg">Swap Panel</CardTitle>
        <CardDescription>
          ledgerとamountを先に選び、詳細設定はAdvancedで必要時のみ編集できます。
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {props.configError ? (
          <p className="rounded-lg bg-rose-50 px-3 py-2 text-xs text-rose-800">
            config error: {props.configError}
          </p>
        ) : null}
        <Tabs
          value={props.tab}
          onValueChange={(value) => {
            if (value === "unwrap" || value === "wrap") {
              props.onTabChange(value);
            }
          }}
        >
          <TabsList>
            <TabsTrigger value="wrap">
              <Sparkles className="mr-1 size-4" />
              Wrap
            </TabsTrigger>
            <TabsTrigger value="unwrap">
              <ArrowDownUp className="mr-1 size-4" />
              Unwrap
            </TabsTrigger>
          </TabsList>

          <TabsContent value="unwrap" className="space-y-3">
            <AssetSelector
              value={props.unwrapForm.assetId}
              options={props.assetOptions}
              selectPlaceholder="asset を選択"
              onChange={(assetId) =>
                props.onUnwrapChange({
                  ...props.unwrapForm,
                  assetId,
                })
              }
            />
            <div className="rounded-xl border border-zinc-200 bg-zinc-50/70 p-4">
              <p className="text-xs font-semibold text-zinc-600">Amount</p>
              <Input
                className="mt-2 h-12 text-lg"
                placeholder="0"
                value={props.unwrapForm.amount}
                onChange={(event) =>
                  props.onUnwrapChange({
                    ...props.unwrapForm,
                    amount: event.target.value,
                  })
                }
              />
              <Button
                className="mt-3 w-full h-11"
                onClick={props.onSubmitUnwrap}
                disabled={
                  props.submitLoading ||
                  !props.walletConnected ||
                  props.configError !== null
                }
              >
                {props.submitLoading ? "Submitting..." : "Submit Unwrap"}
              </Button>
            </div>
            <details className="rounded-xl border border-zinc-200 bg-white p-3">
              <summary className="cursor-pointer text-sm font-medium text-zinc-700">
                Advanced
              </summary>
              <div className="mt-3 grid gap-2 sm:grid-cols-2">
                <Input
                  placeholder="recipient principal"
                  value={props.unwrapForm.recipient}
                  onChange={(event) =>
                    props.onUnwrapChange({
                      ...props.unwrapForm,
                      recipient: event.target.value,
                    })
                  }
                />
              </div>
            </details>
            <p className="text-xs text-zinc-600">
              recipient は Advanced で必ず確認してください。
            </p>
          </TabsContent>

          <TabsContent value="wrap" className="space-y-3">
            <AssetSelector
              value={props.wrapForm.assetId}
              options={props.assetOptions}
              selectPlaceholder="asset を選択"
              onChange={(assetId) =>
                props.onWrapChange({
                  ...props.wrapForm,
                  assetId,
                })
              }
            />
            <div className="rounded-xl border border-zinc-200 bg-zinc-50/70 p-4">
              <div className="flex items-center justify-between gap-3">
                <p className="text-xs font-semibold text-zinc-600">Amount</p>
                <p className="text-xs text-zinc-500">
                  Balance: {props.wrapBalanceText ?? "-"}
                </p>
              </div>
              <div className="relative mt-2">
                <Input
                  className="h-12 pr-16 text-lg"
                  placeholder="0"
                  value={props.wrapForm.amount}
                  onChange={(event) =>
                    props.onWrapChange({
                      ...props.wrapForm,
                      amount: event.target.value,
                    })
                  }
                />
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  className="absolute right-1 top-1/2 -translate-y-1/2"
                  onClick={() =>
                    props.onWrapChange({
                      ...props.wrapForm,
                      amount: props.wrapMaxAmountText ?? props.wrapForm.amount,
                    })
                  }
                  disabled={props.wrapMaxAmountText === null || props.wrapMaxAmountText === "0"}
                >
                  MAX
                </Button>
              </div>
              <Button
                className="mt-3 w-full h-11"
                onClick={props.onSubmitWrap}
                disabled={
                  props.submitLoading ||
                  !props.walletConnected ||
                  props.configError !== null ||
                  props.wrapNonceStatus !== "ready" ||
                  props.wrapGasEstimateStatus !== "ready"
                }
              >
                {props.submitLoading ? "Submitting..." : "Submit Wrap"}
              </Button>
            </div>
            <details className="rounded-xl border border-zinc-200 bg-white p-3">
              <summary className="cursor-pointer text-sm font-medium text-zinc-700">
                Advanced
              </summary>
              <div className="mt-3 space-y-3">
                <div className="space-y-1">
                  <p className="text-xs font-semibold text-zinc-600">EVM Recipient</p>
                  <Input
                    placeholder="evm recipient (0x..)"
                    value={props.wrapForm.evmRecipient}
                    onChange={(event) =>
                      props.onWrapChange({
                        ...props.wrapForm,
                        evmRecipient: event.target.value,
                      })
                    }
                  />
                </div>
                <div className="space-y-1">
                  <p className="text-xs font-semibold text-zinc-600">Gas Limit</p>
                  <Input
                    placeholder="gas limit"
                    value={props.wrapForm.gasLimit}
                    readOnly
                  />
                </div>
                <div className="space-y-1">
                  <p className="text-xs font-semibold text-zinc-600">Charged Gas Price</p>
                  <Input
                    placeholder="charged gas price (gwei)"
                    value={
                      props.wrapChargedGasPriceWei === null
                        ? ""
                        : `${formatWeiToGwei2(BigInt(props.wrapChargedGasPriceWei))} gwei`
                    }
                    readOnly
                  />
                </div>
                <div className="space-y-1">
                  <p className="text-xs font-semibold text-zinc-600">Max Priority Fee Per Gas</p>
                  <Input
                    placeholder="max priority fee per gas (gwei)"
                    value={
                      props.wrapMaxPriorityFeePerGasWei === null
                        ? ""
                        : `${formatWeiToGwei2(BigInt(props.wrapMaxPriorityFeePerGasWei))} gwei`
                    }
                    readOnly
                  />
                </div>
              </div>
            </details>
            <p className="rounded-lg bg-zinc-50 px-3 py-2 text-xs text-zinc-700">
              nonce: {
                props.wrapNonceStatus === "loading"
                  ? "自動取得中..."
                  : props.wrapNonceStatus === "ready"
                    ? props.wrapForm.evmNonce
                    : props.wrapNonceStatus === "error"
                      ? `失敗 (${props.wrapNonceError ?? "wrap.nonce_failed"})`
                      : "-"
              }
            </p>
            <p className="rounded-lg bg-zinc-50 px-3 py-2 text-xs text-zinc-700">
              gas estimate: {
                props.wrapGasEstimateStatus === "estimating"
                  ? "自動見積中..."
                  : props.wrapGasEstimateStatus === "ready"
                    ? props.wrapForm.gasLimit
                    : props.wrapGasEstimateStatus === "error"
                      ? `失敗 (${props.wrapGasEstimateError ?? "wrap.gas_estimate_failed"})`
                      : "-"
              }
            </p>
            <p className="break-all rounded-lg bg-zinc-50 px-3 py-2 font-mono text-xs text-zinc-600">
              request_id: {props.lastSubmittedWrapRequestId ?? props.wrapPreviewRequestId ?? "(入力待ち)"}
            </p>
            <p className="rounded-lg bg-emerald-50 px-3 py-2 text-xs text-emerald-900">
              {props.wrapFeeEstimateText ?? "estimated fee: -"}
            </p>
            <p className="rounded-lg bg-zinc-50 px-3 py-2 text-xs text-zinc-700">
              flow: {wrapStepLabel(props.wrapActionStep)}
            </p>
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>
  );
}
