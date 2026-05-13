"use client";

// どこで: wrapper dashboard
// 何を: /swap 風 shell・wallet modal・consoleを束ねる
// なぜ: 既存ビジネスロジックを保ったまま UI 骨格を刷新するため

import { useEffect, useMemo, useState, type ReactElement } from "react";
import { ConsoleCard } from "@/components/dashboard-ui/console-card";
import { KasaneShell } from "@/components/dashboard-ui/kasane-shell";
import { ManageTokensDrawer } from "@/components/dashboard-ui/manage-tokens-drawer";
import { RequestStatusModal } from "@/components/dashboard-ui/request-status-modal";
import { WalletConnectModal } from "@/components/dashboard-ui/wallet-connect-modal";
import type { ActiveTab, StatusPanelView, WrapActionStep } from "@/components/dashboard-ui/types";
import { applySelectedAsset } from "@/lib/icp-token-list";
import { useKasaneTxTracker } from "@/lib/hooks/use-kasane-tx-tracker";
import { useLedgerBalance } from "@/lib/hooks/use-ledger-balance";
import { useManageTokens } from "@/lib/hooks/use-manage-tokens";
import { useStatusTracker } from "@/lib/hooks/use-status-tracker";
import { useUnwrapBalance } from "@/lib/hooks/use-unwrap-balance";
import { useWrapperActions } from "@/lib/hooks/use-wrapper-actions";
import { useWrapperForms } from "@/lib/hooks/use-wrapper-forms";
import type { loadConfig } from "@/lib/config";
import { formatTokenAmount } from "@/lib/wrap-flow";
import { useWallet } from "@/lib/wallet/use-wallet";

export type WrapperDashboardConfigState = {
  cfg: ReturnType<typeof loadConfig> | null;
  configError: string | null;
};

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
): ReactElement {
  const wallet = useWallet();
  const [tab, setTab] = useState<ActiveTab>("wrap");
  const [drawerOpen, setDrawerOpen] = useState(false);
  const [walletModalOpen, setWalletModalOpen] = useState(false);
  const [metaMaskModalOpen, setMetaMaskModalOpen] = useState(false);
  const [manageTokensOpen, setManageTokensOpen] = useState(false);
  const tracker = useStatusTracker();

  const manageTokens = useManageTokens(cfg?.icpTokenListUrl ?? null, wallet.oisySession?.principalText ?? null);
  const txTracker = useKasaneTxTracker({
    rpcUrl: cfg?.kasaneRpcUrl ?? null,
    explorerBaseUrl: cfg?.kasaneBlockExplorerUrl ?? null,
  });
  const [wrapActionStep, setWrapActionStep] = useState<WrapActionStep>("idle");
  const forms = useWrapperForms({
    walletPrincipalText: wallet.oisySession?.principalText ?? null,
    wrapCanisterId: cfg?.wrapCanisterId ?? "",
    evmWrapFactory: cfg?.evmWrapFactory ?? "",
    wrapActionStep,
  });
  const wrapBalance = useLedgerBalance({
    ledgerCanisterId: forms.wrapForm.assetId,
    ownerPrincipalText: wallet.oisySession?.principalText ?? null,
  });
  const unwrapBalance = useUnwrapBalance({
    assetId: forms.unwrapForm.assetId,
    callerEvmAddressHex: wallet.metaMaskSession?.accountAddress ?? null,
  });
  const actions = useWrapperActions({
    cfg,
    configError,
    oisySession: wallet.oisySession,
    oisyCapabilities: wallet.oisyCapabilities,
    metaMaskSession: wallet.metaMaskSession,
    getCaller: wallet.getCaller,
    forms,
    tracker,
    onRequestIdInput: onOpenRequest,
    onMetaMaskTransactionSubmitted: (transactionHash) => {
      setMetaMaskModalOpen(true);
      txTracker.setMessage(null);
      txTracker.setAutoPolling(true);
      void txTracker.refreshTransaction(transactionHash);
    },
    onWrapActionStepChange: setWrapActionStep,
  });

  useEffect(() => {
    if (statusModalOpen && activeRequestId !== null) {
      void actions.queryAndStartPolling(activeRequestId);
    }
  }, [activeRequestId, actions, statusModalOpen]);

  useEffect(() => {
    if (!statusModalOpen || !metaMaskModalOpen) {
      return;
    }
    setMetaMaskModalOpen(false);
    txTracker.setAutoPolling(false);
    txTracker.setTransaction(null);
    txTracker.setMessage(null);
  }, [metaMaskModalOpen, statusModalOpen, txTracker]);

  const wrapMaxAmountText = useMemo(() => {
    if (wrapBalance.balanceValue === null || wrapBalance.decimals === null) {
      return null;
    }
    const feeAmount = actions.wrapFeeEstimate?.feeLedgerCanister === forms.wrapForm.assetId
      ? actions.wrapFeeEstimate.chargedFeeE8s
      : 0n;
    const maxAmount = wrapBalance.balanceValue > feeAmount ? wrapBalance.balanceValue - feeAmount : 0n;
    return formatTokenAmount(maxAmount, wrapBalance.decimals);
  }, [actions.wrapFeeEstimate, forms.wrapForm.assetId, wrapBalance.balanceValue, wrapBalance.decimals]);

  const modalStatus: StatusPanelView | null = statusModalOpen
    ? tracker.status
    : txTracker.transaction
      ? { kind: "transaction", ...txTracker.transaction }
      : null;
  const modalLabel = statusModalOpen ? activeRequestId : txTracker.transaction?.transactionHash ?? null;
  const walletLabel = wallet.oisySession?.principalText ?? wallet.metaMaskSession?.accountAddress ?? "Connect Wallet";
  const selectedAssetId = tab === "wrap" ? forms.wrapForm.assetId : forms.unwrapForm.assetId;

  function selectAsset(assetId: string): void {
    const next = applySelectedAsset({
      tab,
      assetId,
      wrapForm: forms.wrapForm,
      unwrapForm: forms.unwrapForm,
    });
    forms.setWrapForm(next.wrapForm);
    forms.setUnwrapForm(next.unwrapForm);
    setManageTokensOpen(false);
  }

  return (
    <KasaneShell
      drawerOpen={drawerOpen}
      onDrawerClose={() => setDrawerOpen(false)}
      onDrawerOpen={() => setDrawerOpen(true)}
      onWalletClick={() => setWalletModalOpen(true)}
      walletLabel={walletLabel}
    >
      <div className="flex w-full max-w-[72rem] flex-col gap-5 lg:flex-row lg:items-start lg:justify-center">
        <div className="w-full lg:flex-1">
          <ManageTokensDrawer
            error={manageTokens.error}
            loading={manageTokens.loading}
            mode="mobile"
            onRefresh={() => void manageTokens.refresh()}
            onSelectAsset={selectAsset}
            onToggleMobile={() => setManageTokensOpen((current) => !current)}
            open={manageTokensOpen}
            rows={manageTokens.rows}
            selectedAssetId={selectedAssetId}
          />
          <ConsoleCard
            assetOptions={manageTokens.assetOptions}
            configError={configError}
            lastSubmittedWrapRequestId={actions.lastSubmittedWrapRequestId}
            onOpenWallet={() => setWalletModalOpen(true)}
            onSubmitUnwrap={() => void actions.submitUnwrap()}
            onSubmitWrap={() => void actions.submitWrap()}
            onTabChange={setTab}
            onUnwrapChange={forms.setUnwrapForm}
            onWrapChange={forms.setWrapForm}
            submitLoading={actions.submitLoading}
            tab={tab}
            unwrapForm={forms.unwrapForm}
            wallet={wallet}
            wrapActionStep={actions.wrapActionStep}
            wrapChargedGasPriceWei={actions.wrapGasDetails?.chargedGasPriceWei.toString() ?? null}
            wrapFeeEstimateText={actions.wrapFeeEstimateText}
            wrapForm={forms.wrapForm}
            wrapGasEstimateError={forms.wrapGasEstimateError}
            wrapGasEstimateStatus={forms.wrapGasEstimateStatus}
            wrapMaxAmountText={wrapMaxAmountText}
            wrapMaxPriorityFeePerGasWei={actions.wrapGasDetails?.maxPriorityFeePerGasWei.toString() ?? null}
            wrapNonceError={forms.wrapNonceError}
            wrapNonceStatus={forms.wrapNonceStatus}
            wrapPreviewRequestId={forms.wrapPreviewRequestId}
            wrapBalanceText={wrapBalance.balanceText}
            unwrapBalanceText={unwrapBalance.balanceText}
          />
        </div>
        <div className="hidden lg:block">
          <ManageTokensDrawer
            error={manageTokens.error}
            loading={manageTokens.loading}
            mode="desktop"
            onRefresh={() => void manageTokens.refresh()}
            onSelectAsset={selectAsset}
            onToggleMobile={() => undefined}
            open
            rows={manageTokens.rows}
            selectedAssetId={selectedAssetId}
          />
        </div>
      </div>

      <WalletConnectModal
        onClose={() => setWalletModalOpen(false)}
        onConnectMetaMask={() => void wallet.connectMetaMask()}
        onConnectOisy={() => void wallet.connectOisy()}
        onDisconnectMetaMask={() => wallet.disconnectMetaMask()}
        onDisconnectOisy={() => void wallet.disconnectOisy()}
        open={walletModalOpen}
        wallet={wallet}
      />
      <RequestStatusModal
        message={statusModalOpen ? tracker.message : tracker.message ?? txTracker.message}
        onClose={() => {
          if (statusModalOpen) {
            onCloseRequest();
            return;
          }
          setMetaMaskModalOpen(false);
          txTracker.setAutoPolling(false);
        }}
        onRetry={() => void actions.retryUnwrap()}
        onWithdraw={() => void actions.withdraw()}
        open={statusModalOpen || metaMaskModalOpen}
        requestIdLabel={modalLabel}
        retryLoading={actions.retryLoading}
        status={modalStatus}
        statusLoading={statusModalOpen ? tracker.statusLoading : txTracker.loading}
        walletConnected={wallet.oisySession !== null && wallet.oisyCapabilities.wrapCanisterSupported}
        withdrawLoading={actions.withdrawLoading}
      />
    </KasaneShell>
  );
}
