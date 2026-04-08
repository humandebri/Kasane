"use client";

// どこで: wrapperダッシュボード / 何を: amount中心のWrap/Unwrap送信と状態追跡を統合 / なぜ: 主要導線を1画面で最短操作にするため

import { Suspense, lazy, useEffect, useMemo, useState, type ReactElement } from "react";
import { HeaderBar } from "@/components/dashboard-ui/header-bar";
import { SwapPanel } from "@/components/dashboard-ui/swap-panel";
import type { ActiveTab, WrapActionStep } from "@/components/dashboard-ui/types";
import { useAssetCatalog } from "@/lib/hooks/use-asset-catalog";
import { useLedgerBalance } from "@/lib/hooks/use-ledger-balance";
import { useWrapperActions } from "@/lib/hooks/use-wrapper-actions";
import { useRecentRequests } from "@/lib/hooks/use-recent-requests";
import { useStatusTracker } from "@/lib/hooks/use-status-tracker";
import { useWrapperForms } from "@/lib/hooks/use-wrapper-forms";
import type { loadConfig } from "@/lib/config";
import { resolveJunoSatelliteId } from "@/lib/config";
import { formatTokenAmount } from "@/lib/wrap-flow";
import { useWallet } from "@/lib/wallet/use-wallet";

const HistoryPanel = lazy(async () => {
  const module = await import("@/components/dashboard-ui/history-panel");
  return { default: module.HistoryPanel };
});

const RequestStatusModal = lazy(async () => {
  const module = await import("@/components/dashboard-ui/request-status-modal");
  return { default: module.RequestStatusModal };
});

export type WrapperDashboardConfigState = {
  cfg: ReturnType<typeof loadConfig> | null;
  configError: string | null;
};

function HistoryPanelFallback(): ReactElement {
  return (
    <section className="rounded-2xl border border-emerald-100 bg-white p-6">
      <h2 className="text-lg font-semibold text-zinc-900">Recent Requests</h2>
      <p className="mt-2 text-sm text-zinc-500">Loading history...</p>
    </section>
  );
}

function RequestStatusModalFallback(props: { requestId: string | null }): ReactElement | null {
  if (props.requestId === null) {
    return null;
  }
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-950/40 px-4 py-6 backdrop-blur-sm">
      <div className="w-full max-w-2xl rounded-2xl border border-emerald-100 bg-white shadow-2xl">
        <div className="border-b border-zinc-100 px-5 py-4">
          <p className="text-xs font-semibold uppercase tracking-[0.2em] text-emerald-700">Request Status</p>
          <p className="mt-2 break-all font-mono text-xs text-zinc-600">{props.requestId}</p>
        </div>
        <div className="px-5 py-4">
          <p className="text-sm text-zinc-600">Loading request status...</p>
        </div>
      </div>
    </div>
  );
}

export function WrapperDashboard(
  {
    cfg,
    configError,
    activeRequestId,
    statusModalOpen,
    onOpenRequest,
    onCloseRequest,
  }: WrapperDashboardConfigState & {
    activeRequestId: string | null;
    statusModalOpen: boolean;
    onOpenRequest: (requestId: string) => void;
    onCloseRequest: () => void;
  },
) {
  const wallet = useWallet();

  const [tab, setTab] = useState<ActiveTab>("wrap");
  const assetCatalog = useAssetCatalog();
  const recentRequests = useRecentRequests({
    principalText: wallet.session?.principalText ?? null,
    getIdentity: wallet.getIdentity,
    satelliteId: resolveJunoSatelliteId(),
  });

  const tracker = useStatusTracker();
  const [wrapActionStep, setWrapActionStep] = useState<WrapActionStep>("idle");
  const forms = useWrapperForms({
    walletPrincipalText: wallet.session?.principalText ?? null,
    wrapCanisterId: cfg?.wrapCanisterId ?? "",
    evmWrapFactory: cfg?.evmWrapFactory ?? "",
    wrapActionStep,
  });
  const wrapBalance = useLedgerBalance({
    ledgerCanisterId: forms.wrapForm.assetId,
    ownerPrincipalText: wallet.session?.principalText ?? null,
  });
  const actions = useWrapperActions({
    cfg,
    configError,
    walletSession: wallet.session,
    getIdentity: wallet.getIdentity,
    forms,
    tracker,
    onRequestIdInput: onOpenRequest,
    onRequestSubmitted: (entry) => recentRequests.save(entry),
    onWrapActionStepChange: setWrapActionStep,
  });

  useEffect(() => {
    if (!statusModalOpen || activeRequestId === null) {
      return;
    }
    void actions.queryAndStartPolling(activeRequestId);
  }, [activeRequestId, statusModalOpen]);
  const wrapMaxAmountText = useMemo(() => {
    const balanceValue = wrapBalance.balanceValue;
    const decimals = wrapBalance.decimals;
    if (balanceValue === null || decimals === null) {
      return null;
    }
    const feeEstimate = actions.wrapFeeEstimate;
    if (feeEstimate === null) {
      return formatTokenAmount(balanceValue, decimals);
    }
    const feeAmount = feeEstimate.feeLedgerCanister === forms.wrapForm.assetId
      ? feeEstimate.chargedFeeE8s
      : 0n;
    const maxAmount = balanceValue > feeAmount ? balanceValue - feeAmount : 0n;
    return formatTokenAmount(maxAmount, decimals);
  }, [actions.wrapFeeEstimate, forms.wrapForm.assetId, wrapBalance.balanceValue, wrapBalance.decimals]);

  return (
    <main className="mx-auto flex min-h-screen w-full max-w-7xl flex-col gap-5 px-4 py-7 sm:px-8">
      <HeaderBar
        wallet={wallet}
        onConnectGoogle={() => void wallet.connectGoogle()}
        onConnectInternetIdentity={() => void wallet.connectInternetIdentity()}
        onDisconnect={() => void wallet.disconnect()}
      />
      <section className="grid gap-5 lg:grid-cols-[1.95fr_0.8fr_0.8fr] lg:items-start">
        <SwapPanel
          tab={tab}
          unwrapForm={forms.unwrapForm}
          wrapForm={forms.wrapForm}
          wrapActionStep={actions.wrapActionStep}
          wrapGasEstimateStatus={forms.wrapGasEstimateStatus}
          wrapGasEstimateError={forms.wrapGasEstimateError}
          wrapNonceStatus={forms.wrapNonceStatus}
          wrapNonceError={forms.wrapNonceError}
          wrapFeeEstimateText={actions.wrapFeeEstimateText}
          wrapPreviewRequestId={forms.wrapPreviewRequestId}
          lastSubmittedWrapRequestId={actions.lastSubmittedWrapRequestId}
          wrapBalanceText={wrapBalance.balanceText}
          wrapMaxAmountText={wrapMaxAmountText}
          wrapChargedGasPriceWei={actions.wrapGasDetails?.chargedGasPriceWei.toString() ?? null}
          wrapMaxPriorityFeePerGasWei={actions.wrapGasDetails?.maxPriorityFeePerGasWei.toString() ?? null}
          submitLoading={actions.submitLoading}
          walletConnected={wallet.session !== null}
          configError={configError}
          assetOptions={assetCatalog.assetOptions}
          onTabChange={setTab}
          onUnwrapChange={forms.setUnwrapForm}
          onWrapChange={forms.setWrapForm}
          onSubmitUnwrap={() => void actions.submitUnwrap()}
          onSubmitWrap={() => void actions.submitWrap()}
        />
        <Suspense fallback={<HistoryPanelFallback />}>
          <HistoryPanel
            history={recentRequests.history}
            loading={recentRequests.loading}
            error={recentRequests.error}
            walletConnected={wallet.session !== null}
            onOpen={(requestId) => {
              onOpenRequest(requestId);
            }}
          />
        </Suspense>
      </section>
      {statusModalOpen ? (
        <Suspense fallback={<RequestStatusModalFallback requestId={activeRequestId} />}>
          <RequestStatusModal
            open={statusModalOpen}
            requestId={activeRequestId}
            status={tracker.status}
            statusLoading={tracker.statusLoading}
            message={tracker.message}
            walletConnected={wallet.session !== null}
            retryLoading={actions.retryLoading}
            withdrawLoading={actions.withdrawLoading}
            onClose={onCloseRequest}
            onRetry={() => void actions.retryUnwrap()}
            onWithdraw={() => void actions.withdraw()}
          />
        </Suspense>
      ) : null}
    </main>
  );
}
