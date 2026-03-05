"use client";

// どこで: wrapperダッシュボード / 何を: amount中心のWrap/Unwrap送信と状態追跡を統合 / なぜ: 主要導線を1画面で最短操作にするため

import { useMemo, useState } from "react";
import { HistoryPanel } from "@/components/dashboard-ui/history-panel";
import { HeaderBar } from "@/components/dashboard-ui/header-bar";
import { StatusPanel } from "@/components/dashboard-ui/status-panel";
import { SwapPanel } from "@/components/dashboard-ui/swap-panel";
import type { ActiveTab, HistoryEntry } from "@/components/dashboard-ui/types";
import { useWrapperActions } from "@/lib/hooks/use-wrapper-actions";
import { useStatusTracker } from "@/lib/hooks/use-status-tracker";
import { useWrapperForms } from "@/lib/hooks/use-wrapper-forms";
import { loadConfig } from "@/lib/config";
import { useWallet } from "@/lib/wallet/use-wallet";

function resolveConfig(): {
  cfg: ReturnType<typeof loadConfig> | null;
  configError: string | null;
} {
  try {
    return { cfg: loadConfig(), configError: null };
  } catch (error) {
    const message = error instanceof Error ? error.message : "config.invalid";
    return { cfg: null, configError: message };
  }
}

export function WrapperDashboard() {
  const { cfg, configError } = useMemo(resolveConfig, []);
  const wallet = useWallet();

  const [tab, setTab] = useState<ActiveTab>("unwrap");
  const [requestIdInput, setRequestIdInput] = useState("");
  const [history, setHistory] = useState<HistoryEntry[]>([]);

  const tracker = useStatusTracker();
  const forms = useWrapperForms({
    walletPrincipalText: wallet.session?.principalText ?? null,
    wrapCanisterId: cfg?.wrapCanisterId ?? "",
  });
  const actions = useWrapperActions({
    cfg,
    configError,
    walletSession: wallet.session,
    forms,
    tracker,
    onRequestIdInput: setRequestIdInput,
    onRequestSubmitted: (entry) => {
      setHistory((current) => [entry, ...current].slice(0, 20));
    },
  });

  return (
    <main className="mx-auto flex min-h-screen w-full max-w-6xl flex-col gap-5 px-4 py-7 sm:px-8">
      <HeaderBar
        wallet={wallet}
        host={cfg?.icHost ?? "(config missing)"}
        gatewayCanisterId={cfg?.evmGatewayCanisterId ?? "(config missing)"}
        onConnectInternetIdentity={() => void wallet.connect("ii")}
        onConnectOisy={() => void wallet.connect("oisy")}
        onDisconnect={() => void wallet.disconnect()}
      />
      <section className="grid gap-5 lg:grid-cols-[1.4fr_1fr]">
        <SwapPanel
          tab={tab}
          unwrapForm={forms.unwrapForm}
          wrapForm={forms.wrapForm}
          wrapActionStep={actions.wrapActionStep}
          wrapFeeEstimateText={actions.wrapFeeEstimateText}
          unwrapPreviewRequestId={forms.unwrapPreviewRequestId}
          wrapPreviewRequestId={forms.wrapPreviewRequestId}
          submitLoading={actions.submitLoading}
          walletConnected={wallet.session !== null}
          configError={configError}
          onTabChange={setTab}
          onUnwrapChange={forms.setUnwrapForm}
          onWrapChange={forms.setWrapForm}
          onSubmitUnwrap={() => void actions.submitUnwrap()}
          onSubmitWrap={() => void actions.submitWrap()}
        />
        <StatusPanel
          requestIdInput={requestIdInput}
          status={tracker.status}
          statusLoading={tracker.statusLoading}
          message={tracker.message}
          walletConnected={wallet.session !== null}
          withdrawLoading={actions.withdrawLoading}
          onChangeRequestId={setRequestIdInput}
          onQuery={() => void actions.queryAndStartPolling(requestIdInput)}
          onWithdraw={() => void actions.withdraw()}
        />
      </section>
      <HistoryPanel
        history={history}
        onQuery={(requestId) => {
          setRequestIdInput(requestId);
          void actions.queryAndStartPolling(requestId);
        }}
      />
    </main>
  );
}
