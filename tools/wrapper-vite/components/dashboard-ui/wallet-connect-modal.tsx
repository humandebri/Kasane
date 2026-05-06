// どこで: wrapper wallet modal
// 何を: Oisy / MetaMask の接続と切断を tile UI で提供
// なぜ: header の wallet pill から接続導線を一元化するため

import type { ReactElement } from "react";
import { Button } from "@/components/ui/button";
import type { DashboardWalletState } from "./types";

function WalletTile(props: {
  title: string;
  subtitle?: string;
  disabled?: boolean;
  danger?: boolean;
  cta: string;
  onClick: () => void;
}): ReactElement {
  return (
    <div className="rounded-3xl border border-slate-200 bg-white p-4 shadow-sm">
      <div>
        <p className="text-sm font-semibold text-slate-950">{props.title}</p>
        {props.subtitle ? (
          <p className="mt-1 text-xs leading-5 text-slate-500">{props.subtitle}</p>
        ) : null}
      </div>
      <Button
        className={props.danger ? "mt-4 w-full bg-rose-600 hover:bg-rose-500" : "mt-4 w-full"}
        disabled={props.disabled}
        onClick={props.onClick}
      >
        {props.cta}
      </Button>
    </div>
  );
}

export function WalletConnectModal(props: {
  open: boolean;
  wallet: DashboardWalletState;
  onClose: () => void;
  onConnectOisy: () => void;
  onDisconnectOisy: () => void;
  onConnectMetaMask: () => void;
  onDisconnectMetaMask: () => void;
}): ReactElement | null {
  if (!props.open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/55 px-4 py-6 backdrop-blur-sm" onClick={props.onClose}>
      <div className="w-full max-w-2xl rounded-[2rem] bg-[#f7faff] p-5 shadow-2xl" onClick={(event) => event.stopPropagation()}>
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.24em] text-slate-500">Wallet</p>
            <h2 className="mt-2 text-2xl font-semibold tracking-tight text-slate-950">Connect wallet</h2>
            <p className="mt-2 text-sm text-slate-500">Oisy handles signer identity. MetaMask stays available for direct EVM submission.</p>
          </div>
          <Button onClick={props.onClose} size="sm" variant="outline">Close</Button>
        </div>

        <div className="mt-5 grid gap-4 md:grid-cols-2">
          <WalletTile
            cta={props.wallet.oisySession ? "Disconnect Oisy" : props.wallet.oisyConnecting ? "Connecting..." : "Connect Oisy"}
            danger={props.wallet.oisySession !== null}
            disabled={props.wallet.oisyConnecting}
            onClick={props.wallet.oisySession ? props.onDisconnectOisy : props.onConnectOisy}
            title="Oisy"
          />
          <WalletTile
            cta={props.wallet.metaMaskSession ? "Clear MetaMask" : props.wallet.metaMaskConnecting ? "Connecting..." : "Connect MetaMask"}
            danger={props.wallet.metaMaskSession !== null}
            disabled={props.wallet.metaMaskConnecting || !props.wallet.metaMaskAvailable}
            onClick={props.wallet.metaMaskSession ? props.onDisconnectMetaMask : props.onConnectMetaMask}
            subtitle={props.wallet.metaMaskAvailable ? undefined : "Extension not detected in this browser."}
            title="MetaMask"
          />
        </div>

        {props.wallet.oisySession ? (
          <p className="mt-4 rounded-2xl border border-sky-100 bg-white px-4 py-3 font-mono text-xs text-slate-700">
            oisy principal: {props.wallet.oisySession.principalText}
          </p>
        ) : null}
        {props.wallet.metaMaskSession ? (
          <p className="mt-3 rounded-2xl border border-slate-200 bg-white px-4 py-3 font-mono text-xs text-slate-700">
            metamask account: {props.wallet.metaMaskSession.accountAddress}
          </p>
        ) : null}
        {props.wallet.error ? (
          <p className="mt-3 rounded-2xl border border-rose-200 bg-rose-50 px-4 py-3 text-xs text-rose-700">
            wallet error: {props.wallet.error}
          </p>
        ) : null}
      </div>
    </div>
  );
}
