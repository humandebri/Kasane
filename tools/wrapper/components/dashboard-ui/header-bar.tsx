// どこで: dashboard header / 何を: 接続状態とネットワーク情報を表示 / なぜ: 送信主体と接続先を常に明示するため

import { Wallet } from "lucide-react";
import type { ReactElement } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { DashboardWalletState } from "./types";

export function HeaderBar(props: {
  wallet: DashboardWalletState;
  host: string;
  gatewayCanisterId: string;
  onConnectInternetIdentity: () => void;
  onConnectOisy: () => void;
  onDisconnect: () => void;
}): ReactElement {
  const sourceText = props.wallet.session
    ? props.wallet.session.source.toUpperCase()
    : "DISCONNECTED";

  return (
    <header className="rounded-2xl border border-emerald-100 bg-white/85 p-5 shadow-sm backdrop-blur">
      <div className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.2em] text-emerald-700">
            Kasane
          </p>
          <h1 className="mt-1 text-2xl font-semibold tracking-tight text-zinc-900">
            Wrap / Unwrap Console
          </h1>
          <p className="mt-2 text-xs text-zinc-600">
            host: {props.host} / gateway: {props.gatewayCanisterId}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="neutral">
            <Wallet className="mr-1 size-3" />
            {sourceText}
          </Badge>
          <Button
            size="sm"
            onClick={props.onConnectInternetIdentity}
            disabled={props.wallet.connecting}
          >
            Connect II
          </Button>
          <Button
            size="sm"
            variant="secondary"
            onClick={props.onConnectOisy}
            disabled={props.wallet.connecting || !props.wallet.oisyAvailable}
          >
            Connect Oisy
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={props.onDisconnect}
            disabled={!props.wallet.session}
          >
            Disconnect
          </Button>
        </div>
      </div>
      {props.wallet.session ? (
        <p className="mt-3 truncate rounded-lg bg-zinc-50 px-3 py-2 font-mono text-xs text-zinc-700">
          principal: {props.wallet.session.principalText}
        </p>
      ) : null}
      {props.wallet.error ? (
        <p className="mt-2 text-xs text-rose-700">wallet error: {props.wallet.error}</p>
      ) : null}
    </header>
  );
}
