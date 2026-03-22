"use client";

// どこで: wrapperダッシュボード / 何を: amount中心のWrap/Unwrap送信と状態追跡を統合 / なぜ: 主要導線を1画面で最短操作にするため

import { useMemo, useState } from "react";
import { HistoryPanel } from "@/components/dashboard-ui/history-panel";
import { HeaderBar } from "@/components/dashboard-ui/header-bar";
import { RequestStatusModal } from "@/components/dashboard-ui/request-status-modal";
import { SwapPanel } from "@/components/dashboard-ui/swap-panel";
import type { ActiveTab, WrapActionStep } from "@/components/dashboard-ui/types";
import { useAssetCatalog } from "@/lib/hooks/use-asset-catalog";
import { useLedgerBalance } from "@/lib/hooks/use-ledger-balance";
import { useWrapperActions } from "@/lib/hooks/use-wrapper-actions";
import { useRecentRequests } from "@/lib/hooks/use-recent-requests";
import { useStatusTracker } from "@/lib/hooks/use-status-tracker";
import { useWrapperForms } from "@/lib/hooks/use-wrapper-forms";
import { resolveJunoSatelliteId, type loadConfig } from "@/lib/config";
import { formatTokenAmount } from "@/lib/wrap-flow";
import { useWallet } from "@/lib/wallet/use-wallet";

export type WrapperDashboardConfigState = {
  cfg: ReturnType<typeof loadConfig> | null;
  configError: string | null;
};

export function WrapperDashboard({ cfg, configError }: WrapperDashboardConfigState) {
  const wallet = useWallet();

  const [tab, setTab] = useState<ActiveTab>("wrap");
  const [activeRequestId, setActiveRequestId] = useState<string | null>(null);
  const [statusModalOpen, setStatusModalOpen] = useState(false);
  const assetCatalog = useAssetCatalog();
  const recentRequests = useRecentRequests({
    identity: wallet.session?.identity ?? null,
    principalText: wallet.session?.principalText ?? null,
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
    forms,
    tracker,
    onRequestIdInput: (requestId) => {
      setActiveRequestId(requestId);
      setStatusModalOpen(true);
    },
    onRequestSubmitted: (entry) => {
      void recentRequests.save(entry);
    },
    onWrapActionStepChange: setWrapActionStep,
    onWrapSucceeded: () => {
      void wrapBalance.refresh();
    },
  });

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
        onConnectInternetIdentity={() => void wallet.connect("ii")}
        onConnectOisy={() => void wallet.connect("oisy")}
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
        <HistoryPanel
          history={recentRequests.history}
          loading={recentRequests.loading}
          error={recentRequests.error}
          walletConnected={wallet.session !== null}
          onOpen={(requestId) => {
            setActiveRequestId(requestId);
            setStatusModalOpen(true);
            void actions.queryAndStartPolling(requestId);
          }}
        />
      </section>
      <RequestStatusModal
        open={statusModalOpen}
        requestId={activeRequestId}
        status={tracker.status}
        statusLoading={tracker.statusLoading}
        message={tracker.message}
        walletConnected={wallet.session !== null}
        retryLoading={actions.retryLoading}
        withdrawLoading={actions.withdrawLoading}
        onClose={() => setStatusModalOpen(false)}
        onRetry={() => void actions.retryUnwrap()}
        onWithdraw={() => void actions.withdraw()}
      />
    </main>
  );
}
