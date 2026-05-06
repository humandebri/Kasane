// どこで: ICP token list adapter
// 何を: 外部 token list payload を Kasane 用 row と selector option に正規化
// なぜ: source schema 依存を UI へ漏らさず、Manage Tokens drawer を単純化するため

import type { ActiveTab, UnwrapFormState, WrapFormState } from "@/components/dashboard-ui/types";
import type { AssetOption } from "@/lib/asset-catalog";
import { principalTextToBytes } from "@/lib/principal";

type UnknownRecord = Record<string, unknown>;

export type ManageTokenRow = {
  assetId: string;
  symbol: string | null;
  name: string | null;
  logo: string | null;
  searchText: string;
  balanceText: string | null;
};

function isUnknownRecord(value: unknown): value is UnknownRecord {
  return typeof value === "object" && value !== null;
}

function readString(record: UnknownRecord, keys: string[]): string | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.trim() !== "") {
      return value.trim();
    }
  }
  return null;
}

function readItems(payload: unknown): unknown[] {
  if (Array.isArray(payload)) {
    return payload;
  }
  if (!isUnknownRecord(payload)) {
    throw new Error("token_list.payload_invalid");
  }
  const content = payload.content;
  if (Array.isArray(content)) {
    return content;
  }
  const tokens = payload.tokens;
  if (Array.isArray(tokens)) {
    return tokens;
  }
  throw new Error("token_list.payload_invalid");
}

function normalizeRow(item: unknown): ManageTokenRow {
  if (!isUnknownRecord(item)) {
    throw new Error("token_list.entry_invalid");
  }
  const assetId = readString(item, ["assetId", "ledgerId", "canisterId", "id"]);
  if (assetId === null) {
    throw new Error("token_list.asset_id_missing");
  }
  principalTextToBytes(assetId);
  const symbol = readString(item, ["symbol", "ticker"]);
  const name = readString(item, ["name", "displayName"]);
  const logo = readString(item, ["logo", "logoUrl", "icon"]);
  return {
    assetId,
    symbol,
    name,
    logo,
    searchText: [symbol, name, assetId].filter((value): value is string => value !== null).join(" ").toLowerCase(),
    balanceText: null,
  };
}

export function normalizeIcpTokenList(payload: unknown): ManageTokenRow[] {
  const out: ManageTokenRow[] = [];
  const seen = new Set<string>();
  for (const item of readItems(payload)) {
    const row = normalizeRow(item);
    if (seen.has(row.assetId)) {
      continue;
    }
    seen.add(row.assetId);
    out.push(row);
  }
  return out;
}

export function toManageTokenOptions(rows: ManageTokenRow[]): AssetOption[] {
  return rows.map((row) => ({
    assetId: row.assetId,
    label: row.symbol ?? row.name ?? row.assetId,
    source: "token_list",
  }));
}

export function applySelectedAsset(args: {
  tab: ActiveTab;
  assetId: string;
  wrapForm: WrapFormState;
  unwrapForm: UnwrapFormState;
}): {
  wrapForm: WrapFormState;
  unwrapForm: UnwrapFormState;
} {
  if (args.tab === "wrap") {
    return {
      wrapForm: { ...args.wrapForm, assetId: args.assetId },
      unwrapForm: args.unwrapForm,
    };
  }
  return {
    wrapForm: args.wrapForm,
    unwrapForm: { ...args.unwrapForm, assetId: args.assetId },
  };
}
