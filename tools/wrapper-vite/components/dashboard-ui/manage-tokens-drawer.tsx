// どこで: wrapper dashboard right drawer
// 何を: ICP 側 token 一覧を検索・refresh し、現在の asset selector に反映
// なぜ: ICPSwap の token 管理導線に寄せつつ、Kasane では token browser に用途を絞るため

import { RefreshCw, Search } from "lucide-react";
import { useMemo, useState, type ReactElement } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { ManageTokenRow } from "@/lib/icp-token-list";

function TokenAvatar(props: { row: ManageTokenRow }): ReactElement {
  if (props.row.logo !== null) {
    return (
      <img
        alt={props.row.symbol ?? props.row.name ?? props.row.assetId}
        className="size-10 rounded-full border border-slate-200 object-cover"
        src={props.row.logo}
      />
    );
  }
  return (
    <div className="grid size-10 place-items-center rounded-full bg-slate-200 text-sm font-semibold text-slate-700">
      {(props.row.symbol ?? props.row.name ?? "?").slice(0, 1).toUpperCase()}
    </div>
  );
}

export function ManageTokensDrawer(props: {
  mode: "desktop" | "mobile";
  open: boolean;
  loading: boolean;
  error: string | null;
  rows: ManageTokenRow[];
  selectedAssetId: string;
  onToggleMobile: () => void;
  onRefresh: () => void;
  onSelectAsset: (assetId: string) => void;
}): ReactElement {
  const [searchText, setSearchText] = useState("");
  const filteredRows = useMemo(() => {
    const normalized = searchText.trim().toLowerCase();
    if (normalized === "") {
      return props.rows;
    }
    return props.rows.filter((row) => row.searchText.includes(normalized));
  }, [props.rows, searchText]);
  const visible = props.mode === "desktop" || props.open;

  return (
    <>
      {props.mode === "mobile" ? (
        <div className="mb-4 flex items-center justify-between rounded-[1.5rem] border border-white/50 bg-white/88 p-3 shadow-sm lg:hidden">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.16em] text-slate-500">Manage Tokens</p>
            <p className="text-sm font-semibold text-slate-900">ICP-side ICRC token browser</p>
          </div>
          <Button className="rounded-full" onClick={props.onToggleMobile} size="sm" variant="outline">
            {props.open ? "Hide" : "Show"}
          </Button>
        </div>
      ) : null}
      <aside
        className={
          !visible
            ? "hidden"
            : props.mode === "mobile"
              ? "w-full max-w-[24rem] shrink-0 lg:hidden"
              : "w-full max-w-[24rem] shrink-0"
        }
      >
        <section className="rounded-[2rem] border border-white/55 bg-white/94 p-4 shadow-[0_22px_70px_rgba(15,23,42,0.12)]">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="text-[11px] font-semibold uppercase tracking-[0.18em] text-slate-500">Manage Tokens</p>
              <p className="mt-1 text-lg font-semibold text-slate-950">ICP ICRC Tokens</p>
              <p className="mt-1 text-xs text-slate-500">Browse tokens and apply one to the current wrap or unwrap form.</p>
            </div>
            <Button className="rounded-full" disabled={props.loading} onClick={props.onRefresh} size="sm" variant="outline">
              <RefreshCw className={props.loading ? "size-4 animate-spin" : "size-4"} />
            </Button>
          </div>

          <div className="mt-4">
            <div className="relative">
              <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-slate-400" />
              <Input
                className="h-11 rounded-2xl border-slate-200 bg-slate-50 pl-10"
                onChange={(event) => setSearchText(event.target.value)}
                placeholder="Search token or principal..."
                value={searchText}
              />
            </div>
          </div>

          {props.error !== null ? (
            <p className="mt-4 rounded-2xl border border-rose-200 bg-rose-50 px-4 py-3 text-xs text-rose-700">
              token list error: {props.error}
            </p>
          ) : null}

          <div className="mt-4 max-h-[34rem] space-y-3 overflow-auto pr-1">
            {props.loading && props.rows.length === 0 ? (
              <p className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-6 text-sm text-slate-500">
                Loading tokens...
              </p>
            ) : filteredRows.length === 0 ? (
              <p className="rounded-2xl border border-slate-200 bg-slate-50 px-4 py-6 text-sm text-slate-500">
                No tokens found.
              </p>
            ) : filteredRows.map((row) => {
              const selected = row.assetId === props.selectedAssetId;
              return (
                <button
                  className={selected
                    ? "w-full rounded-[1.4rem] border border-[#1e2c57] bg-[#f3f7ff] p-4 text-left shadow-sm"
                    : "w-full rounded-[1.4rem] border border-slate-200 bg-slate-50/80 p-4 text-left transition hover:border-slate-300 hover:bg-white"}
                  key={row.assetId}
                  onClick={() => props.onSelectAsset(row.assetId)}
                  type="button"
                >
                  <div className="flex items-start gap-3">
                    <TokenAvatar row={row} />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center justify-between gap-3">
                        <p className="truncate text-sm font-semibold text-slate-950">
                          {row.name ?? row.symbol ?? row.assetId}
                        </p>
                        <span className="shrink-0 text-[11px] font-medium text-slate-500">
                          {row.balanceText ?? "-"}
                        </span>
                      </div>
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        </section>
      </aside>
    </>
  );
}
