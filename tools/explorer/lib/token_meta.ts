// どこで: Explorerトークン補助層 / 何を: ERC-20 symbol/decimals取得にTTL付き上限キャッシュを適用 / なぜ: メモリ増大とRPC増幅を防ぐため

import { getRpcCallObject } from "./rpc";
import { parseAddressHex, toHexLower } from "./hex";

export type TokenMetaView = {
  symbol: string | null;
  decimals: number | null;
};

type CacheEntry = {
  value: TokenMetaView;
  expiresAtMs: number;
  isError: boolean;
};

type TokenMetaFetcher = (normalizedAddressHex: string) => Promise<TokenMetaView>;

const SYMBOL_SELECTOR = Buffer.from("95d89b41", "hex");
const DECIMALS_SELECTOR = Buffer.from("313ce567", "hex");
const MAX_CACHE_ENTRIES = 1000;
const SUCCESS_TTL_MS = 24 * 60 * 60 * 1000;
const ERROR_TTL_MS = 5 * 60 * 1000;
const MAX_CONCURRENT_FETCHES = 5;

const cache = new Map<string, CacheEntry>();
const inFlight = new Map<string, Promise<TokenMetaView>>();
const semaphoreQueue: Array<() => void> = [];
let activeFetches = 0;
let nowProvider = () => Date.now();
let tokenMetaFetcher: TokenMetaFetcher = fetchTokenMetaFromRpc;

export async function getTokenMeta(addressHex: string): Promise<TokenMetaView> {
  const normalized = toHexLower(parseAddressHex(addressHex));
  const cached = getFreshCacheEntry(normalized);
  if (cached) {
    return cached.value;
  }
  const pending = inFlight.get(normalized);
  if (pending) {
    return pending;
  }
  const task = fetchAndCache(normalized).finally(() => {
    inFlight.delete(normalized);
  });
  inFlight.set(normalized, task);
  return task;
}

async function fetchAndCache(normalizedAddressHex: string): Promise<TokenMetaView> {
  await acquireSemaphoreSlot();
  try {
    const value = await tokenMetaFetcher(normalizedAddressHex);
    setCacheEntry(normalizedAddressHex, {
      value,
      expiresAtMs: nowProvider() + SUCCESS_TTL_MS,
      isError: false,
    });
    return value;
  } catch {
    const fallback: TokenMetaView = { symbol: null, decimals: null };
    setCacheEntry(normalizedAddressHex, {
      value: fallback,
      expiresAtMs: nowProvider() + ERROR_TTL_MS,
      isError: true,
    });
    return fallback;
  } finally {
    releaseSemaphoreSlot();
  }
}

async function fetchTokenMetaFromRpc(addressHex: string): Promise<TokenMetaView> {
  const address = parseAddressHex(addressHex);
  const [symbolOut, decimalsOut] = await Promise.all([
    getRpcCallObject({
      to: [address],
      gas: [],
      value: [],
      max_priority_fee_per_gas: [],
      data: [SYMBOL_SELECTOR],
      from: [],
      max_fee_per_gas: [],
      chain_id: [],
      nonce: [],
      tx_type: [],
      access_list: [],
      gas_price: [],
    }),
    getRpcCallObject({
      to: [address],
      gas: [],
      value: [],
      max_priority_fee_per_gas: [],
      data: [DECIMALS_SELECTOR],
      from: [],
      max_fee_per_gas: [],
      chain_id: [],
      nonce: [],
      tx_type: [],
      access_list: [],
      gas_price: [],
    }),
  ]);

  const symbol = "Ok" in symbolOut && symbolOut.Ok.status === 1 ? decodeSymbol(symbolOut.Ok.return_data) : null;
  const decimals = "Ok" in decimalsOut && decimalsOut.Ok.status === 1 ? decodeDecimals(decimalsOut.Ok.return_data) : null;
  return { symbol, decimals };
}

function getFreshCacheEntry(normalizedAddressHex: string): CacheEntry | null {
  const entry = cache.get(normalizedAddressHex);
  if (!entry) {
    return null;
  }
  if (entry.expiresAtMs <= nowProvider()) {
    cache.delete(normalizedAddressHex);
    return null;
  }
  touchCacheEntry(normalizedAddressHex, entry);
  return entry;
}

function setCacheEntry(normalizedAddressHex: string, entry: CacheEntry): void {
  cache.delete(normalizedAddressHex);
  cache.set(normalizedAddressHex, entry);
  evictOldestIfNeeded();
}

function touchCacheEntry(normalizedAddressHex: string, entry: CacheEntry): void {
  cache.delete(normalizedAddressHex);
  cache.set(normalizedAddressHex, entry);
}

function evictOldestIfNeeded(): void {
  while (cache.size > MAX_CACHE_ENTRIES) {
    const oldestKey = cache.keys().next().value;
    if (typeof oldestKey !== "string") {
      break;
    }
    cache.delete(oldestKey);
  }
}

async function acquireSemaphoreSlot(): Promise<void> {
  if (activeFetches < MAX_CONCURRENT_FETCHES) {
    activeFetches += 1;
    return;
  }
  await new Promise<void>((resolve) => {
    semaphoreQueue.push(resolve);
  });
  activeFetches += 1;
}

function releaseSemaphoreSlot(): void {
  if (activeFetches > 0) {
    activeFetches -= 1;
  }
  const next = semaphoreQueue.shift();
  if (next) {
    next();
  }
}

function decodeSymbol(data: Uint8Array): string | null {
  if (data.length === 0) {
    return null;
  }
  if (data.length >= 64) {
    const offset = wordToNumber(data.subarray(0, 32));
    if (offset === null || data.length < offset + 32) {
      return decodeBytes32Symbol(data);
    }
    const len = wordToNumber(data.subarray(offset, offset + 32));
    if (len === null || len < 0 || data.length < offset + 32 + len) {
      return decodeBytes32Symbol(data);
    }
    const raw = data.subarray(offset + 32, offset + 32 + len);
    const text = bytesToAscii(raw).trim();
    return text.length > 0 ? text : null;
  }
  return decodeBytes32Symbol(data);
}

function decodeBytes32Symbol(data: Uint8Array): string | null {
  if (data.length < 32) {
    return null;
  }
  const raw = data.subarray(0, 32);
  let end = raw.length;
  for (let i = 0; i < raw.length; i += 1) {
    if (raw[i] === 0) {
      end = i;
      break;
    }
  }
  const text = bytesToAscii(raw.subarray(0, end)).trim();
  return text.length > 0 ? text : null;
}

function decodeDecimals(data: Uint8Array): number | null {
  if (data.length < 32) {
    return null;
  }
  let out = 0n;
  for (const value of data.subarray(0, 32)) {
    out = (out << 8n) + BigInt(value);
  }
  if (out < 0n || out > 255n) {
    return null;
  }
  return Number(out);
}

function wordToNumber(word: Uint8Array): number | null {
  if (word.length !== 32) {
    return null;
  }
  let value = 0n;
  for (const part of word) {
    value = (value << 8n) + BigInt(part);
  }
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    return null;
  }
  return Number(value);
}

function bytesToAscii(data: Uint8Array): string {
  const chars: string[] = [];
  for (const byte of data) {
    if (byte >= 32 && byte <= 126) {
      chars.push(String.fromCharCode(byte));
    }
  }
  return chars.join("");
}

function resetForTest(): void {
  cache.clear();
  inFlight.clear();
  semaphoreQueue.length = 0;
  activeFetches = 0;
  nowProvider = () => Date.now();
  tokenMetaFetcher = fetchTokenMetaFromRpc;
}

export const tokenMetaTestHooks = {
  decodeSymbol,
  decodeDecimals,
  resetForTest,
  setNowProviderForTest: (provider: () => number): void => {
    nowProvider = provider;
  },
  setFetcherForTest: (fetcher: TokenMetaFetcher): void => {
    tokenMetaFetcher = fetcher;
  },
  getCacheSizeForTest: (): number => cache.size,
  getInFlightSizeForTest: (): number => inFlight.size,
  getIsErrorForTest: (addressHex: string): boolean | null => {
    const normalized = toHexLower(parseAddressHex(addressHex));
    const entry = cache.get(normalized);
    return entry ? entry.isError : null;
  },
  constants: {
    MAX_CACHE_ENTRIES,
    SUCCESS_TTL_MS,
    ERROR_TTL_MS,
    MAX_CONCURRENT_FETCHES,
  },
};
