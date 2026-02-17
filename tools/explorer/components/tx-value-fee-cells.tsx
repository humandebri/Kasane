"use client";

// どこで: HomeのLatest Transactions行 / 何を: Amount/Txn Fee をクライアントから canister query で後読み / なぜ: SSRのN+1 RPC負荷とCORS制約を避けるため

import { useEffect, useState } from "react";
import { Actor, HttpAgent } from "@dfinity/agent";
import type { IDL } from "@dfinity/candid";
import { TableCell } from "./ui/table";
import { formatIcpAmountFromWei } from "../lib/format";
import { bytesToBigInt, parseHex } from "../lib/hex";

type Props = {
  txHashHex: string;
  canisterId: string | null;
  icHost: string;
};

type TxMetrics = {
  valueWei: bigint | null;
  feeWei: bigint | null;
};

type CacheEntry = {
  expiresAtMs: number;
  value: TxMetrics;
};

const CACHE_TTL_MS = 30_000;
const MAX_CONCURRENT = 4;
const MAX_CACHE_ENTRIES = 500;
const TX_ID_PATTERN = /^0x[0-9a-fA-F]{64}$/;

const cache = new Map<string, CacheEntry>();
const inflight = new Map<string, Promise<TxMetrics>>();
const waiters: Array<() => void> = [];
let activeCount = 0;
let cachedActor: ExplorerTxMetricsActor | null = null;
let cachedActorKey: string | null = null;

export function TxValueFeeCells({ txHashHex, canisterId, icHost }: Props) {
  const [metrics, setMetrics] = useState<TxMetrics | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetchTxMetrics(txHashHex, canisterId, icHost)
      .then((out) => {
        if (!cancelled) {
          setMetrics(out);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setMetrics({ valueWei: null, feeWei: null });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [txHashHex, canisterId, icHost]);

  const amountText =
    metrics?.valueWei === null || metrics?.valueWei === undefined ? "N/A" : formatIcpAmountFromWei(metrics.valueWei);
  const feeText =
    metrics?.feeWei === null || metrics?.feeWei === undefined ? "N/A" : formatIcpAmountFromWei(metrics.feeWei);

  return (
    <>
      <TableCell className="font-mono text-xs">{amountText}</TableCell>
      <TableCell className="font-mono text-xs">{feeText}</TableCell>
    </>
  );
}

async function fetchTxMetrics(txHashHex: string, canisterId: string | null, icHost: string): Promise<TxMetrics> {
  if (!canisterId || !TX_ID_PATTERN.test(txHashHex)) {
    return { valueWei: null, feeWei: null };
  }

  const key = `${canisterId}:${icHost}:${txHashHex.toLowerCase()}`;
  const now = Date.now();
  evictExpiredEntries(now);
  const cached = cache.get(key);
  if (cached && cached.expiresAtMs > now) {
    return cached.value;
  }
  const existing = inflight.get(key);
  if (existing) {
    return existing;
  }
  const promise = withLimiter(async () => {
    const actor = await getActor(canisterId, icHost);
    const txId = parseHex(txHashHex);
    const [txOpt, receiptResult] = await Promise.all([
      actor.rpc_eth_get_transaction_by_tx_id(txId),
      actor.get_receipt(txId),
    ]);
    const tx = txOpt.length === 0 ? null : txOpt[0];
    const valueWei = tx?.decoded.length === 1 ? bytesToBigInt(tx.decoded[0].value) : null;
    const feeWei = "Ok" in receiptResult ? receiptResult.Ok.total_fee : null;
    const out: TxMetrics = { valueWei, feeWei };
    cache.set(key, { value: out, expiresAtMs: Date.now() + CACHE_TTL_MS });
    enforceCacheLimit();
    return out;
  });
  inflight.set(key, promise);
  try {
    return await promise;
  } finally {
    inflight.delete(key);
  }
}

async function withLimiter<T>(fn: () => Promise<T>): Promise<T> {
  if (activeCount >= MAX_CONCURRENT) {
    await new Promise<void>((resolve) => {
      waiters.push(resolve);
    });
  }
  activeCount += 1;
  try {
    return await fn();
  } finally {
    activeCount -= 1;
    const next = waiters.shift();
    if (next) {
      next();
    }
  }
}

type RpcTxDecodedView = {
  value: Uint8Array;
};

type RpcTxView = {
  decoded: [] | [RpcTxDecodedView];
};

type ReceiptView = {
  total_fee: bigint;
};

type LookupError = { NotFound: null } | { Pending: null } | { Pruned: { pruned_before_block: bigint } };
type Result<T, E> = { Ok: T } | { Err: E };

type ExplorerTxMetricsActor = {
  rpc_eth_get_transaction_by_tx_id: (txId: Uint8Array) => Promise<[] | [RpcTxView]>;
  get_receipt: (txId: Uint8Array) => Promise<Result<ReceiptView, LookupError>>;
};

async function getActor(canisterId: string, icHost: string): Promise<ExplorerTxMetricsActor> {
  const cacheKey = `${canisterId}@${icHost}`;
  if (cachedActor && cachedActorKey === cacheKey) {
    return cachedActor;
  }
  const agent = new HttpAgent({ host: icHost });
  cachedActor = Actor.createActor<ExplorerTxMetricsActor>(idlFactory, { agent, canisterId });
  cachedActorKey = cacheKey;
  return cachedActor;
}

const idlFactory: IDL.InterfaceFactory = ({ IDL }) => {
  const lookupError = IDL.Variant({
    NotFound: IDL.Null,
    Pruned: IDL.Record({ pruned_before_block: IDL.Nat64 }),
    Pending: IDL.Null,
  });
  const receiptView = IDL.Record({
    total_fee: IDL.Nat,
  });
  const rpcTxDecodedView = IDL.Record({
    value: IDL.Vec(IDL.Nat8),
  });
  const rpcTxView = IDL.Record({
    decoded: IDL.Opt(rpcTxDecodedView),
  });
  return IDL.Service({
    get_receipt: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Variant({ Ok: receiptView, Err: lookupError })], ["query"]),
    rpc_eth_get_transaction_by_tx_id: IDL.Func([IDL.Vec(IDL.Nat8)], [IDL.Opt(rpcTxView)], ["query"]),
  });
};

function evictExpiredEntries(now: number): void {
  for (const [key, entry] of cache.entries()) {
    if (entry.expiresAtMs <= now) {
      cache.delete(key);
    }
  }
}

function enforceCacheLimit(): void {
  while (cache.size > MAX_CACHE_ENTRIES) {
    const oldestKey = cache.keys().next().value;
    if (typeof oldestKey !== "string") {
      break;
    }
    cache.delete(oldestKey);
  }
}

export const txValueFeeCellsTestHooks = {
  isValidTxIdHex(value: string): boolean {
    return TX_ID_PATTERN.test(value);
  },
  getCacheSizeForTest(): number {
    return cache.size;
  }
};
