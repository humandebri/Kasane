"use client";

// どこで: Logs検索フォーム / 何を: Enter/フォーカスアウトでクエリに反映 / なぜ: 入力途中の不要な検索とエラー表示を避けるため

import { useEffect, useState } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";

type LogsFilters = {
  fromBlock: string;
  toBlock: string;
  address: string;
  topic0: string;
  window: string;
};

type Props = {
  initialFilters: LogsFilters;
};

export function LogsSearchForm({ initialFilters }: Props) {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const [filters, setFilters] = useState<LogsFilters>(initialFilters);

  useEffect(() => {
    setFilters(initialFilters);
  }, [initialFilters]);

  const commit = (): void => {
    if (!isValidNumericOrEmpty(filters.fromBlock)) {
      return;
    }
    if (!isValidNumericOrEmpty(filters.toBlock)) {
      return;
    }
    if (!isValidNumericOrEmpty(filters.window)) {
      return;
    }
    const nextQuery = buildQuery(filters);
    const nextQueryText = nextQuery.toString();
    const currentQueryText = searchParams.toString();
    if (nextQueryText === currentQueryText) {
      return;
    }
    const href = nextQueryText === "" ? pathname : `${pathname}?${nextQueryText}`;
    router.replace(href, { scroll: false });
  };

  return (
    <form
      className="grid grid-cols-1 gap-2 md:grid-cols-5"
      onSubmit={(event) => {
        event.preventDefault();
        commit();
      }}
    >
      <input
        name="fromBlock"
        placeholder="fromBlock"
        value={filters.fromBlock}
        onChange={(event) => {
          setFilters((prev) => ({ ...prev, fromBlock: event.target.value }));
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commit();
          }
        }}
        className="h-9 rounded-md border px-3 text-sm"
      />
      <input
        name="toBlock"
        placeholder="toBlock"
        value={filters.toBlock}
        onChange={(event) => {
          setFilters((prev) => ({ ...prev, toBlock: event.target.value }));
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commit();
          }
        }}
        className="h-9 rounded-md border px-3 text-sm"
      />
      <input
        name="address"
        placeholder="address"
        value={filters.address}
        onChange={(event) => {
          setFilters((prev) => ({ ...prev, address: event.target.value }));
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commit();
          }
        }}
        className="h-9 rounded-md border px-3 text-sm font-mono"
      />
      <input
        name="topic0"
        placeholder="topic0"
        value={filters.topic0}
        onChange={(event) => {
          setFilters((prev) => ({ ...prev, topic0: event.target.value }));
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commit();
          }
        }}
        className="h-9 rounded-md border px-3 text-sm font-mono"
      />
      <input
        name="window"
        placeholder="window"
        value={filters.window}
        onChange={(event) => {
          setFilters((prev) => ({ ...prev, window: event.target.value }));
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commit();
          }
        }}
        className="h-9 rounded-md border px-3 text-sm"
      />
    </form>
  );
}

function buildQuery(filters: LogsFilters): URLSearchParams {
  const query = new URLSearchParams();
  const fromBlock = filters.fromBlock.trim();
  const toBlock = filters.toBlock.trim();
  const address = filters.address.trim();
  const topic0 = filters.topic0.trim();
  const window = filters.window.trim();
  if (fromBlock !== "") query.set("fromBlock", fromBlock);
  if (toBlock !== "") query.set("toBlock", toBlock);
  if (address !== "") query.set("address", address);
  if (topic0 !== "") query.set("topic0", topic0);
  if (window !== "") query.set("window", window);
  return query;
}

function isValidNumericOrEmpty(value: string): boolean {
  const trimmed = value.trim();
  return trimmed === "" || /^\d+$/.test(trimmed);
}
