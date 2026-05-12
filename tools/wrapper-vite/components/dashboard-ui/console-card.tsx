// どこで: wrapper console card
// 何を: /swap 風の中央 card に Wrap / Unwrap を統合表示
// なぜ: 既存ロジックを維持したまま UI 骨格だけ刷新するため

import { ArrowDownUp, ChevronDown, Wallet } from "lucide-react";
import { useState, type ReactElement } from "react";
import { AssetSelector } from "@/components/dashboard-ui/asset-selector";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { AssetOption } from "@/lib/asset-catalog";
import { formatWeiToGwei2 } from "@/lib/wrap-flow";
import type {
  ActiveTab,
  DashboardWalletState,
  UnwrapFormState,
  WrapActionStep,
  WrapFormState,
} from "./types";

function actionLabel(step: WrapActionStep, loading: boolean, tab: ActiveTab): string {
  if (loading) return "Submitting...";
  if (tab === "wrap") {
    if (step === "approving_asset" || step === "approving_fee") return "Approving...";
    if (step === "quoting") return "Quoting...";
  }
  return tab === "wrap" ? "Submit Wrap" : "Submit Unwrap";
}

export function ConsoleCard(props: {
  tab: ActiveTab;
  wallet: DashboardWalletState;
  unwrapForm: UnwrapFormState;
  wrapForm: WrapFormState;
  wrapActionStep: WrapActionStep;
  wrapFeeEstimateText: string | null;
  wrapPreviewRequestId: string | null;
  lastSubmittedWrapRequestId: string | null;
  wrapBalanceText: string | null;
  unwrapBalanceText: string | null;
  wrapMaxAmountText: string | null;
  wrapChargedGasPriceWei: string | null;
  wrapMaxPriorityFeePerGasWei: string | null;
  wrapGasEstimateStatus: "idle" | "estimating" | "ready" | "error";
  wrapGasEstimateError: string | null;
  wrapNonceStatus: "idle" | "loading" | "ready" | "error";
  wrapNonceError: string | null;
  submitLoading: boolean;
  configError: string | null;
  assetOptions: AssetOption[];
  onOpenWallet: () => void;
  onTabChange: (tab: ActiveTab) => void;
  onUnwrapChange: (next: UnwrapFormState) => void;
  onWrapChange: (next: WrapFormState) => void;
  onSubmitWrap: () => void;
  onSubmitUnwrap: () => void;
}): ReactElement {
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const wrapReady = props.wallet.oisySession !== null && props.wallet.oisyCapabilities.wrapCanisterSupported;
  const unwrapReady = props.wallet.metaMaskSession !== null;

  return (
    <section className="w-full max-w-[36rem] rounded-[2rem] border border-white/60 bg-white/95 p-4 shadow-[0_28px_90px_rgba(15,23,42,0.18)] sm:p-5">
      <div className="flex items-center justify-between gap-3">
        <div className="rounded-full bg-slate-100 p-1">
          {(["wrap", "unwrap"] as const).map((item) => (
            <button
              className={item === props.tab
                ? "rounded-full bg-[#101a37] px-4 py-2 text-sm font-semibold text-white"
                : "rounded-full px-4 py-2 text-sm font-medium text-slate-500"}
              key={item}
              onClick={() => props.onTabChange(item)}
              type="button"
            >
              {item === "wrap" ? "Wrap" : "Unwrap"}
            </button>
          ))}
        </div>
        <Button className="rounded-full" onClick={props.onOpenWallet} size="sm" variant="outline">
          <Wallet className="size-4" />
          Wallet
        </Button>
      </div>

      {props.configError ? (
        <p className="mt-4 rounded-2xl border border-rose-200 bg-rose-50 px-4 py-3 text-xs text-rose-700">
          config error: {props.configError}
        </p>
      ) : null}

      <div className="mt-4 space-y-3">
        <AssetSelector
          onChange={(assetId) => props.tab === "wrap"
            ? props.onWrapChange({ ...props.wrapForm, assetId })
            : props.onUnwrapChange({ ...props.unwrapForm, assetId })}
          options={props.assetOptions}
          selectPlaceholder="Select asset"
          value={props.tab === "wrap" ? props.wrapForm.assetId : props.unwrapForm.assetId}
        />

        <div className="rounded-[1.6rem] bg-[#eef4ff] p-4">
          <div className="flex items-center justify-between text-xs font-semibold text-slate-500">
            <span>{props.tab === "wrap" ? "You send" : "Token"}</span>
            <span>Balance: {props.tab === "wrap" ? props.wrapBalanceText ?? "-" : props.unwrapBalanceText ?? "-"}</span>
          </div>
          <Input
            className="mt-3 h-16 border-0 bg-transparent px-0 text-4xl font-semibold shadow-none outline-hidden"
            onChange={(event) => props.tab === "wrap"
              ? props.onWrapChange({ ...props.wrapForm, amount: event.target.value })
              : props.onUnwrapChange({ ...props.unwrapForm, amount: event.target.value })}
            placeholder="0"
            value={props.tab === "wrap" ? props.wrapForm.amount : props.unwrapForm.amount}
          />
          {props.tab === "wrap" ? (
            <div className="mt-3 flex items-center justify-between gap-3 text-xs text-slate-500">
              <span>{props.wrapFeeEstimateText ?? "Fee quote pending"}</span>
              <button
                className="rounded-full bg-white px-3 py-1 font-semibold text-slate-700 shadow-sm"
                disabled={props.wrapMaxAmountText === null || props.wrapMaxAmountText === "0"}
                onClick={() => props.onWrapChange({ ...props.wrapForm, amount: props.wrapMaxAmountText ?? props.wrapForm.amount })}
                type="button"
              >
                MAX
              </button>
            </div>
          ) : null}
        </div>

        <div className="mx-auto grid size-11 place-items-center rounded-2xl border border-slate-200 bg-white shadow-sm">
          <ArrowDownUp className="size-4 text-slate-500" />
        </div>

        <div className="rounded-[1.6rem] bg-[#f8fbff] p-4">
          {props.tab === "unwrap" ? (
            <>
              <p className="text-xs font-semibold text-slate-500">MetaMask direct unwrap</p>
              <p className="mt-3 text-xs text-slate-500">
                {props.wallet.metaMaskSession?.accountAddress ?? "Connect MetaMask to send direct Kasane transactions."}
              </p>
              <Input
                className="mt-3 h-12 bg-white"
                onChange={(event) => props.onUnwrapChange({ ...props.unwrapForm, recipient: event.target.value })}
                placeholder="recipient principal"
                value={props.unwrapForm.recipient}
              />
            </>
          ) : (
            <>
              <p className="text-xs font-semibold text-slate-500">Destination EVM address</p>
              <Input
                className="mt-3 h-12 bg-white"
                onChange={(event) => props.onWrapChange({ ...props.wrapForm, evmRecipient: event.target.value })}
                placeholder="0x..."
                value={props.wrapForm.evmRecipient}
              />
            </>
          )}
        </div>

        <Button
          className="h-12 w-full rounded-full text-sm font-semibold"
          disabled={props.submitLoading || props.configError !== null || (props.tab === "wrap" ? !wrapReady : !unwrapReady)}
          onClick={props.tab === "wrap" ? props.onSubmitWrap : props.onSubmitUnwrap}
        >
          {actionLabel(props.wrapActionStep, props.submitLoading, props.tab)}
        </Button>

        <button
          className="flex w-full items-center justify-between rounded-2xl border border-slate-200 px-4 py-3 text-sm font-medium text-slate-700"
          onClick={() => setAdvancedOpen((current) => !current)}
          type="button"
        >
          <span>Advanced</span>
          <ChevronDown className={advancedOpen ? "size-4 rotate-180 transition" : "size-4 transition"} />
        </button>

        {advancedOpen ? (
          <div className="space-y-3 rounded-2xl border border-slate-200 bg-slate-50 p-4 text-xs text-slate-600">
            {props.tab === "wrap" ? (
              <>
                <p>nonce: {props.wrapNonceStatus === "ready" ? props.wrapForm.evmNonce : props.wrapNonceError ?? props.wrapNonceStatus}</p>
                <p>gas estimate: {props.wrapGasEstimateStatus === "ready" ? props.wrapForm.gasLimit : props.wrapGasEstimateError ?? props.wrapGasEstimateStatus}</p>
                <p>charged gas price: {props.wrapChargedGasPriceWei ? `${formatWeiToGwei2(BigInt(props.wrapChargedGasPriceWei))} gwei` : "-"}</p>
                <p>max priority fee: {props.wrapMaxPriorityFeePerGasWei ? `${formatWeiToGwei2(BigInt(props.wrapMaxPriorityFeePerGasWei))} gwei` : "-"}</p>
                <p className="break-all font-mono">request_id: {props.lastSubmittedWrapRequestId ?? props.wrapPreviewRequestId ?? "(waiting for input)"}</p>
              </>
            ) : (
              <p>recipient principal remains required for every unwrap request.</p>
            )}
          </div>
        ) : null}
      </div>
    </section>
  );
}
