// どこで: wrapper history page
// 何を: Recent Requests を主画面から分離して単独ページで表示
// なぜ: 初期画面を /swap 風に軽く保ちつつ、履歴導線は維持するため

import type { ReactElement } from "react";
import { HistoryPanel } from "@/components/dashboard-ui/history-panel";
import type { DashboardWalletState, HistoryEntry } from "@/components/dashboard-ui/types";

export function HistoryPage(props: {
  wallet: DashboardWalletState;
  history: HistoryEntry[];
  loading: boolean;
  error: string | null;
  onOpen: (requestId: string) => void;
}): ReactElement {
  return (
    <section className="w-full max-w-3xl">
      <div className="mb-4 rounded-[2rem] border border-white/50 bg-white/90 p-5 shadow-[0_20px_60px_rgba(15,23,42,0.12)]">
        <p className="text-xs font-semibold uppercase tracking-[0.24em] text-slate-500">History</p>
        <h1 className="mt-2 text-2xl font-semibold tracking-tight text-slate-950">Recent Requests</h1>
        <p className="mt-2 text-sm text-slate-500">
          Request history is separated from the main console. MetaMask-only unwrap submissions do not create request IDs here.
        </p>
      </div>
      <HistoryPanel
        error={props.error}
        history={props.history}
        loading={props.loading}
        onOpen={props.onOpen}
        walletConnected={props.wallet.oisySession !== null}
      />
    </section>
  );
}
