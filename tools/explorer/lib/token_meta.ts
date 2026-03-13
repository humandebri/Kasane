// どこで: Explorerトークン補助層 / 何を: ERC-20 metadata取得にTTL付き上限キャッシュを適用 / なぜ: メモリ増大とRPC増幅を防ぐため

import { getRpcCallObject } from "./rpc";
import { parseAddressHex, toHexLower } from "./hex";

export type TokenMetaView = {
  symbol: string | null;
  decimals: number | null;
};

export type ExtendedTokenMetaView = TokenMetaView & {
  name: string | null;
  totalSupplyRaw: bigint | null;
};

type CacheEntry<TValue> = {
  value: TValue;
  expiresAtMs: number;
  isError: boolean;
};

type TokenMetaFetcher = (normalizedAddressHex: string) => Promise<TokenMetaView>;
type ExtendedTokenMetaFetcher = (normalizedAddressHex: string) => Promise<ExtendedTokenMetaView>;

const NAME_SELECTOR = Buffer.from("06fdde03", "hex");
const SYMBOL_SELECTOR = Buffer.from("95d89b41", "hex");
const DECIMALS_SELECTOR = Buffer.from("313ce567", "hex");
const TOTAL_SUPPLY_SELECTOR = Buffer.from("18160ddd", "hex");
const MAX_CACHE_ENTRIES = 1000;
const SUCCESS_TTL_MS = 24 * 60 * 60 * 1000;
const ERROR_TTL_MS = 5 * 60 * 1000;
const MAX_CONCURRENT_FETCHES = 5;

const tokenMetaCache = new Map<string, CacheEntry<TokenMetaView>>();
const tokenMetaInFlight = new Map<string, Promise<TokenMetaView>>();
const extendedTokenMetaCache = new Map<string, CacheEntry<ExtendedTokenMetaView>>();
const extendedTokenMetaInFlight = new Map<string, Promise<ExtendedTokenMetaView>>();
const semaphoreQueue: Array<() => void> = [];
let activeFetches = 0;
let nowProvider = () => Date.now();
let tokenMetaFetcher: TokenMetaFetcher = fetchTokenMetaFromRpc;
let extendedTokenMetaFetcher: ExtendedTokenMetaFetcher = fetchExtendedTokenMetaFromRpc;

export async function getTokenMeta(addressHex: string): Promise<TokenMetaView> {
  const normalized = toHexLower(parseAddressHex(addressHex));
  return getCachedMeta(normalized, tokenMetaCache, tokenMetaInFlight, tokenMetaFetcher, {
    symbol: null,
    decimals: null,
  });
}

export async function getExtendedTokenMeta(addressHex: string): Promise<ExtendedTokenMetaView> {
  const normalized = toHexLower(parseAddressHex(addressHex));
  return getCachedMeta(normalized, extendedTokenMetaCache, extendedTokenMetaInFlight, extendedTokenMetaFetcher, {
    name: null,
    symbol: null,
    decimals: null,
    totalSupplyRaw: null,
  });
}

async function getCachedMeta<TValue>(
  normalizedAddressHex: string,
  cache: Map<string, CacheEntry<TValue>>,
  inFlight: Map<string, Promise<TValue>>,
  fetcher: (normalizedAddressHex: string) => Promise<TValue>,
  fallback: TValue
): Promise<TValue> {
  const cached = getFreshCacheEntry(normalizedAddressHex, cache);
  if (cached) {
    return cached.value;
  }
  const pending = inFlight.get(normalizedAddressHex);
  if (pending) {
    return pending;
  }
  const task = fetchAndCache(normalizedAddressHex, cache, fetcher, fallback).finally(() => {
    inFlight.delete(normalizedAddressHex);
  });
  inFlight.set(normalizedAddressHex, task);
  return task;
}

async function fetchAndCache<TValue>(
  normalizedAddressHex: string,
  cache: Map<string, CacheEntry<TValue>>,
  fetcher: (normalizedAddressHex: string) => Promise<TValue>,
  fallback: TValue
): Promise<TValue> {
  await acquireSemaphoreSlot();
  try {
    const value = await fetcher(normalizedAddressHex);
    setCacheEntry(normalizedAddressHex, cache, {
      value,
      expiresAtMs: nowProvider() + SUCCESS_TTL_MS,
      isError: false,
    });
    return value;
  } catch {
    setCacheEntry(normalizedAddressHex, cache, {
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

async function fetchExtendedTokenMetaFromRpc(addressHex: string): Promise<ExtendedTokenMetaView> {
  const address = parseAddressHex(addressHex);
  const [nameOut, symbolOut, decimalsOut, totalSupplyOut] = await Promise.all([
    getRpcCallObject({
      to: [address],
      gas: [],
      value: [],
      max_priority_fee_per_gas: [],
      data: [NAME_SELECTOR],
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
    getRpcCallObject({
      to: [address],
      gas: [],
      value: [],
      max_priority_fee_per_gas: [],
      data: [TOTAL_SUPPLY_SELECTOR],
      from: [],
      max_fee_per_gas: [],
      chain_id: [],
      nonce: [],
      tx_type: [],
      access_list: [],
      gas_price: [],
    }),
  ]);

  const name = "Ok" in nameOut && nameOut.Ok.status === 1 ? decodeAbiString(nameOut.Ok.return_data) : null;
  const symbol = "Ok" in symbolOut && symbolOut.Ok.status === 1 ? decodeSymbol(symbolOut.Ok.return_data) : null;
  const decimals = "Ok" in decimalsOut && decimalsOut.Ok.status === 1 ? decodeDecimals(decimalsOut.Ok.return_data) : null;
  const totalSupplyRaw =
    "Ok" in totalSupplyOut && totalSupplyOut.Ok.status === 1 ? decodeUint256(totalSupplyOut.Ok.return_data) : null;
  return { name, symbol, decimals, totalSupplyRaw };
}

function getFreshCacheEntry<TValue>(
  normalizedAddressHex: string,
  cache: Map<string, CacheEntry<TValue>>
): CacheEntry<TValue> | null {
  const entry = cache.get(normalizedAddressHex);
  if (!entry) {
    return null;
  }
  if (entry.expiresAtMs <= nowProvider()) {
    cache.delete(normalizedAddressHex);
    return null;
  }
  touchCacheEntry(normalizedAddressHex, cache, entry);
  return entry;
}

function setCacheEntry<TValue>(normalizedAddressHex: string, cache: Map<string, CacheEntry<TValue>>, entry: CacheEntry<TValue>): void {
  cache.delete(normalizedAddressHex);
  cache.set(normalizedAddressHex, entry);
  evictOldestIfNeeded();
}

function touchCacheEntry<TValue>(
  normalizedAddressHex: string,
  cache: Map<string, CacheEntry<TValue>>,
  entry: CacheEntry<TValue>
): void {
  cache.delete(normalizedAddressHex);
  cache.set(normalizedAddressHex, entry);
}

function evictOldestIfNeeded(): void {
  evictOldestFromCache(tokenMetaCache);
  evictOldestFromCache(extendedTokenMetaCache);
}

function evictOldestFromCache<TValue>(cache: Map<string, CacheEntry<TValue>>): void {
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

function decodeAbiString(data: Uint8Array): string | null {
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

function decodeSymbol(data: Uint8Array): string | null {
  return decodeAbiString(data);
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

function decodeUint256(data: Uint8Array): bigint | null {
  if (data.length < 32) {
    return null;
  }
  let out = 0n;
  for (const value of data.subarray(0, 32)) {
    out = (out << 8n) + BigInt(value);
  }
  return out;
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
  tokenMetaCache.clear();
  tokenMetaInFlight.clear();
  extendedTokenMetaCache.clear();
  extendedTokenMetaInFlight.clear();
  semaphoreQueue.length = 0;
  activeFetches = 0;
  nowProvider = () => Date.now();
  tokenMetaFetcher = fetchTokenMetaFromRpc;
  extendedTokenMetaFetcher = fetchExtendedTokenMetaFromRpc;
}

export const tokenMetaTestHooks = {
  decodeAbiString,
  decodeSymbol,
  decodeDecimals,
  decodeUint256,
  resetForTest,
  setNowProviderForTest: (provider: () => number): void => {
    nowProvider = provider;
  },
  setFetcherForTest: (fetcher: TokenMetaFetcher): void => {
    tokenMetaFetcher = fetcher;
  },
  setExtendedFetcherForTest: (fetcher: ExtendedTokenMetaFetcher): void => {
    extendedTokenMetaFetcher = fetcher;
  },
  getCacheSizeForTest: (): number => tokenMetaCache.size,
  getExtendedCacheSizeForTest: (): number => extendedTokenMetaCache.size,
  getInFlightSizeForTest: (): number => tokenMetaInFlight.size + extendedTokenMetaInFlight.size,
  getIsErrorForTest: (addressHex: string): boolean | null => {
    const normalized = toHexLower(parseAddressHex(addressHex));
    const entry = tokenMetaCache.get(normalized);
    return entry ? entry.isError : null;
  },
  getExtendedIsErrorForTest: (addressHex: string): boolean | null => {
    const normalized = toHexLower(parseAddressHex(addressHex));
    const entry = extendedTokenMetaCache.get(normalized);
    return entry ? entry.isError : null;
  },
  constants: {
    MAX_CACHE_ENTRIES,
    SUCCESS_TTL_MS,
    ERROR_TTL_MS,
    MAX_CONCURRENT_FETCHES,
  },
};
