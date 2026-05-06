// どこで: wrapper dashboard shell
// 何を: top header・mobile drawer・中心コンテンツ枠を提供
// なぜ: ICPSwap /swap 骨格へ寄せつつ、Kasane 固有 UI を載せるため

import { Menu, X } from "lucide-react";
import type { ReactElement, ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";
import { Button } from "@/components/ui/button";

function shorten(value: string): string {
  if (value.length <= 18) {
    return value;
  }
  return `${value.slice(0, 8)}...${value.slice(-6)}`;
}

function NavItem(props: { to: string; label: string; onClick?: () => void }): ReactElement {
  const location = useLocation();
  const active = location.pathname === props.to;
  return (
    <Link
      className={
        active
          ? "rounded-full bg-[#182449] px-4 py-2 text-sm font-semibold text-white"
          : "rounded-full px-4 py-2 text-sm font-medium text-slate-300 transition hover:bg-white/6 hover:text-white"
      }
      onClick={props.onClick}
      to={props.to}
    >
      {props.label}
    </Link>
  );
}

export function KasaneShell(props: {
  walletLabel: string;
  onWalletClick: () => void;
  drawerOpen: boolean;
  onDrawerOpen: () => void;
  onDrawerClose: () => void;
  children: ReactNode;
}): ReactElement {
  return (
    <div className="min-h-screen bg-[radial-gradient(circle_at_top,_rgba(114,153,255,0.22),_transparent_30%),linear-gradient(180deg,#0a1022_0%,#0d1730_42%,#eef4ff_42%,#eef4ff_100%)]">
      <header className="sticky top-0 z-40 border-b border-white/8 bg-[#0a1022]/92 backdrop-blur">
        <div className="mx-auto flex h-16 max-w-7xl items-center justify-between px-4 sm:px-6">
          <div className="flex items-center gap-3">
            <Button
              className="size-10 rounded-full border border-white/10 bg-white/5 text-white hover:bg-white/10 md:hidden"
              onClick={props.onDrawerOpen}
              size="sm"
              variant="outline"
            >
              <Menu className="size-4" />
            </Button>
            <Link className="flex items-center gap-3" to="/">
              <div className="grid size-10 place-items-center rounded-2xl bg-linear-to-br from-sky-400 via-cyan-300 to-emerald-300 text-sm font-black text-slate-950">
                K
              </div>
              <div>
                <p className="text-[11px] font-semibold uppercase tracking-[0.24em] text-sky-200/70">Kasane</p>
                <p className="text-sm font-semibold text-white">Wrap / Unwrap Console</p>
              </div>
            </Link>
            <nav className="hidden items-center gap-2 md:flex">
              <NavItem label="Console" to="/" />
              <NavItem label="History" to="/history" />
            </nav>
          </div>
          <button
            className="flex items-center gap-2 rounded-full border border-white/10 bg-white/8 px-3 py-2 text-sm font-medium text-white transition hover:bg-white/12"
            onClick={props.onWalletClick}
            type="button"
          >
            <span className="grid size-7 place-items-center rounded-full bg-white/12 text-[11px] font-bold">
              {props.walletLabel === "Connect Wallet" ? "W" : props.walletLabel[0]}
            </span>
            <span>{shorten(props.walletLabel)}</span>
          </button>
        </div>
      </header>

      {props.drawerOpen ? (
        <div className="fixed inset-0 z-50 bg-slate-950/60 md:hidden" onClick={props.onDrawerClose}>
          <aside
            className="h-full w-72 border-r border-white/10 bg-[#0a1022] p-4 shadow-2xl"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="flex items-center justify-between">
              <p className="text-sm font-semibold text-white">Menu</p>
              <Button
                className="size-9 rounded-full border border-white/10 bg-white/5 text-white hover:bg-white/10"
                onClick={props.onDrawerClose}
                size="sm"
                variant="outline"
              >
                <X className="size-4" />
              </Button>
            </div>
            <div className="mt-6 flex flex-col gap-2">
              <NavItem label="Console" onClick={props.onDrawerClose} to="/" />
              <NavItem label="History" onClick={props.onDrawerClose} to="/history" />
            </div>
          </aside>
        </div>
      ) : null}

      <main className="mx-auto flex min-h-[calc(100vh-4rem)] w-full max-w-7xl items-start justify-center px-4 py-8 sm:px-6">
        {props.children}
      </main>
    </div>
  );
}
