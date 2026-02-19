// どこで: verify入力正規化 / 何を: API入力を厳格に整形・検証 / なぜ: ハッシュ重複判定とセキュリティ境界を安定させるため

import { createHash } from "node:crypto";
import { gzipSync, gunzipSync } from "node:zlib";
import { isAddressHex, normalizeHex, parseHex } from "../hex";
import type { VerifySourceBundle, VerifySubmitInput } from "./types";

const EMPTY_CONSTRUCTOR_ARGS = "0x";
const MAX_SOURCE_FILES = 100;
const MAX_SOURCE_FILE_BYTES = 512 * 1024;
const MAX_CONTRACT_NAME_BYTES = 240;

export function normalizeVerifySubmitInput(raw: unknown): VerifySubmitInput {
  if (!isObject(raw)) {
    throw new Error("input must be an object");
  }
  const chainId = parseInteger(raw.chainId, "chainId", 0, Number.MAX_SAFE_INTEGER);
  const contractAddress = parseAddress(raw.contractAddress);
  const compilerVersion = parseNonEmptyString(raw.compilerVersion, "compilerVersion");
  const optimizerEnabled = parseBoolean(raw.optimizerEnabled, "optimizerEnabled");
  const optimizerRuns = parseInteger(raw.optimizerRuns, "optimizerRuns", 0, 1_000_000);
  const evmVersion = parseOptionalString(raw.evmVersion, "evmVersion");
  const sourceBundle = parseSourceBundle(raw.sourceBundle);
  const contractName = parseContractName(raw.contractName);
  const constructorArgsHex = parseConstructorArgs(raw.constructorArgsHex);
  return {
    chainId,
    contractAddress,
    compilerVersion,
    optimizerEnabled,
    optimizerRuns,
    evmVersion,
    sourceBundle,
    contractName,
    constructorArgsHex,
  };
}

export function canonicalizeVerifyInput(input: VerifySubmitInput): string {
  const sortedSourceKeys = Object.keys(input.sourceBundle).sort();
  const sortedSourceBundle: VerifySourceBundle = {};
  for (const key of sortedSourceKeys) {
    const content = input.sourceBundle[key];
    if (content === undefined) {
      continue;
    }
    sortedSourceBundle[key] = content;
  }
  return JSON.stringify({
    chainId: input.chainId,
    contractAddress: input.contractAddress,
    compilerVersion: input.compilerVersion,
    optimizerEnabled: input.optimizerEnabled,
    optimizerRuns: input.optimizerRuns,
    evmVersion: input.evmVersion,
    sourceBundle: sortedSourceBundle,
    contractName: input.contractName,
    constructorArgsHex: input.constructorArgsHex,
  });
}

export function hashVerifyInput(canonicalJson: string): string {
  return createHash("sha256").update(canonicalJson).digest("hex");
}

export function compressVerifyPayload(canonicalJson: string): Uint8Array {
  return gzipSync(Buffer.from(canonicalJson, "utf8"));
}

export function decompressVerifyPayload(payloadCompressed: Uint8Array): VerifySubmitInput {
  const raw = gunzipSync(Buffer.from(payloadCompressed)).toString("utf8");
  return normalizeVerifySubmitInput(JSON.parse(raw));
}

function parseConstructorArgs(raw: unknown): string {
  if (raw === undefined || raw === null || raw === "") {
    return EMPTY_CONSTRUCTOR_ARGS;
  }
  if (typeof raw !== "string") {
    throw new Error("constructorArgsHex must be a string");
  }
  const normalized = normalizeHex(raw);
  if (normalized === "0x") {
    return normalized;
  }
  parseHex(normalized);
  return normalized.toLowerCase();
}

function parseSourceBundle(raw: unknown): VerifySourceBundle {
  if (!isObject(raw)) {
    throw new Error("sourceBundle must be an object");
  }
  const out: VerifySourceBundle = {};
  let files = 0;
  for (const [key, value] of Object.entries(raw)) {
    const filePath = key.trim();
    if (!filePath) {
      continue;
    }
    if (!isSafeSourcePath(filePath)) {
      throw new Error(`sourceBundle[${filePath}] has invalid path`);
    }
    files += 1;
    if (files > MAX_SOURCE_FILES) {
      throw new Error("sourceBundle file count exceeded");
    }
    const text = extractSourceText(filePath, value);
    if (Buffer.byteLength(text, "utf8") > MAX_SOURCE_FILE_BYTES) {
      throw new Error(`sourceBundle[${filePath}] file too large`);
    }
    out[filePath] = text;
  }
  if (Object.keys(out).length === 0) {
    throw new Error("sourceBundle is empty");
  }
  return out;
}

function extractSourceText(filePath: string, value: unknown): string {
    if (typeof value === "string") {
      return value;
    }
    if (isObject(value) && typeof value.content === "string") {
      return value.content;
    }
  throw new Error(`sourceBundle[${filePath}] must be string or {content}`);
}

function isSafeSourcePath(filePath: string): boolean {
  if (filePath.length > 240) {
    return false;
  }
  if (filePath.includes("..") || filePath.startsWith("/") || filePath.startsWith("\\")) {
    return false;
  }
  return /^[a-zA-Z0-9_./\\-]+$/.test(filePath);
}

function parseAddress(raw: unknown): string {
  if (typeof raw !== "string") {
    throw new Error("contractAddress must be a string");
  }
  if (!isAddressHex(raw)) {
    throw new Error("contractAddress must be 20-byte hex");
  }
  return normalizeHex(raw);
}

function parseBoolean(raw: unknown, name: string): boolean {
  if (typeof raw !== "boolean") {
    throw new Error(`${name} must be boolean`);
  }
  return raw;
}

function parseInteger(raw: unknown, name: string, min: number, max: number): number {
  if (typeof raw !== "number" || !Number.isInteger(raw)) {
    throw new Error(`${name} must be integer`);
  }
  if (raw < min || raw > max) {
    throw new Error(`${name} out of range`);
  }
  return raw;
}

function parseNonEmptyString(raw: unknown, name: string): string {
  if (typeof raw !== "string" || raw.trim() === "") {
    throw new Error(`${name} must be non-empty string`);
  }
  return raw.trim();
}

function parseContractName(raw: unknown): string {
  const value = parseNonEmptyString(raw, "contractName");
  if (Buffer.byteLength(value, "utf8") > MAX_CONTRACT_NAME_BYTES) {
    throw new Error("contractName too long");
  }
  if (!/^[a-zA-Z0-9_./:\\-]+$/.test(value)) {
    throw new Error("contractName has forbidden characters");
  }
  return value;
}

function parseOptionalString(raw: unknown, name: string): string | null {
  if (raw === undefined || raw === null || raw === "") {
    return null;
  }
  if (typeof raw !== "string") {
    throw new Error(`${name} must be string`);
  }
  return raw.trim();
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
