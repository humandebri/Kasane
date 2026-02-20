// どこで: アドレス詳細ページ / 何を: 概要・Tx履歴・ERC-20 Transfer履歴をタブ表示 / なぜ: Etherscan風の調査導線を提供するため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../../components/ui/table";
import { TxValueFeeCells } from "../../../components/tx-value-fee-cells";
import { getAddressView } from "../../../lib/data";
import { getVerifiedContractByAddress, getVerifyBlobById } from "../../../lib/db";
import { loadConfig } from "../../../lib/config";
import { formatIcpAmountFromWei } from "../../../lib/format";
import { isAddressHex, normalizeHex, shortHex } from "../../../lib/hex";
import { decodeSourceBundleFromGzip } from "../../../lib/verify/source_bundle";

export const dynamic = "force-dynamic";

type AddressTab = "tx" | "token";

export default async function AddressPage({
  params,
  searchParams,
}: {
  params: Promise<{ hex: string }>;
  searchParams: Promise<{ cursor?: string; tokenCursor?: string; principal?: string; tab?: string }>;
}) {
  const { hex } = await params;
  const { cursor, tokenCursor, principal, tab } = await searchParams;
  if (!isAddressHex(hex)) {
    notFound();
  }
  const normalizedHex = normalizeHex(hex);
  const currentTab: AddressTab = tab === "token" ? "token" : "tx";
  const cfg = loadConfig(process.env);
  const [data, verified] = await Promise.all([
    getAddressView(normalizedHex, cursor, tokenCursor, principal ?? null),
    getVerifiedContractByAddress(normalizedHex, cfg.verifyDefaultChainId),
  ]);
  const sourceBundle = verified ? await loadSourceBundle(verified.sourceBlobId) : null;
  const canisterId = process.env.EVM_CANISTER_ID ?? null;
  const icHost = process.env.EXPLORER_IC_HOST ?? process.env.INDEXER_IC_HOST ?? "https://icp-api.io";

  return (
    <div className="space-y-4">
      <Card className="border-slate-200 bg-white shadow-sm">
        <CardHeader className="gap-3 border-b border-slate-200">
          <CardTitle className="font-mono text-base break-all">{data.addressHex}</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4 pt-4">
          {data.providedPrincipal ? (
            <div className="rounded-lg border border-sky-200 bg-sky-50 p-3 text-sm">
              derived from principal:{" "}
              <Link href={`/principal/${encodeURIComponent(data.providedPrincipal)}`} className="font-mono text-sky-700 hover:underline">
                {data.providedPrincipal}
              </Link>
            </div>
          ) : null}

          {data.submitterPrincipals.length > 0 ? (
            <div className="rounded-lg border border-slate-200 bg-slate-50 p-3 text-sm">
              <div className="mb-1 font-medium">submit_ic_tx Issuer Principals</div>
              <div className="flex flex-wrap gap-2">
                {data.submitterPrincipals.map((p) => (
                  <span key={p} className="rounded-full border border-slate-300 bg-white px-3 py-1 font-mono text-xs text-slate-800">
                    {p}
                  </span>
                ))}
              </div>
            </div>
          ) : null}

          <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[190px_1fr]">
            <dt className="text-slate-500">Balance (ICP)</dt>
            <dd className="font-mono">{data.balance === null ? "N/A" : formatIcpAmountFromWei(data.balance)}</dd>
            <dt className="text-slate-500">Nonce</dt>
            <dd>{data.nonce === null ? "N/A" : data.nonce.toString()}</dd>
            <dt className="text-slate-500">Code Bytes</dt>
            <dd>{data.codeBytes === null ? "N/A" : data.codeBytes.toString()}</dd>
            <dt className="text-slate-500">Type</dt>
            <dd>
              <Badge variant={data.isContract === true ? "secondary" : "outline"}>
                {data.isContract === null ? "Unknown" : data.isContract ? "Contract" : "EOA"}
              </Badge>
            </dd>
            <dt className="text-slate-500">Verification</dt>
            <dd className="flex items-center gap-2">
              <Badge variant={verified ? "secondary" : "outline"}>{verified ? "Verified" : "Not Verified"}</Badge>
              {!verified ? (
                <Link href={`/verify?address=${encodeURIComponent(normalizedHex)}`} className="text-sky-700 hover:underline">
                  Submit
                </Link>
              ) : null}
            </dd>
            {verified ? (
              <>
                <dt className="text-slate-500">Contract Name</dt>
                <dd>{verified.contractName}</dd>
                <dt className="text-slate-500">Compiler</dt>
                <dd className="font-mono text-xs">{verified.compilerVersion}</dd>
                <dt className="text-slate-500">Optimization</dt>
                <dd>{verified.optimizerEnabled ? `enabled (runs=${verified.optimizerRuns})` : "disabled"}</dd>
                <dt className="text-slate-500">Verify Match</dt>
                <dd>{`runtime=${verified.runtimeMatch ? "ok" : "ng"} / creation=${verified.creationMatch ? "ok" : "ng"}`}</dd>
                <dt className="text-slate-500">Source Files</dt>
                <dd>{sourceBundle ? Object.keys(sourceBundle).length : 0}</dd>
                {sourceBundle ? (
                  <>
                    <dt className="text-slate-500">Source Preview</dt>
                    <dd>
                      <details className="rounded-md border border-slate-200 p-2">
                        <summary className="cursor-pointer text-xs">open</summary>
                        <div className="mt-2 space-y-3">
                          {Object.entries(sourceBundle).map(([path, content]) => (
                            <div key={path}>
                              <div className="font-mono text-xs text-slate-600">{path}</div>
                              <pre className="max-h-48 overflow-auto rounded bg-slate-50 p-2 text-xs">{content}</pre>
                            </div>
                          ))}
                        </div>
                      </details>
                    </dd>
                  </>
                ) : null}
              </>
            ) : null}
          </dl>

          {data.warnings.length > 0 ? (
            <div className="rounded-lg border border-amber-200 bg-amber-50 p-3 text-sm">
              <div className="mb-1 font-medium">Warnings</div>
              <ul className="list-disc pl-5">
                {data.warnings.map((warning) => (
                  <li key={warning}>{warning}</li>
                ))}
              </ul>
            </div>
          ) : null}
        </CardContent>
      </Card>

      <Card className="border-slate-200 bg-white shadow-sm">
        <CardHeader className="gap-2">
          <CardTitle>Activity</CardTitle>
          <div className="flex flex-wrap gap-2 text-sm">
            <Link
              href={buildAddressTabHref(normalizedHex, "tx", data.providedPrincipal)}
              scroll={false}
              className={`rounded-md border px-3 py-1 ${currentTab === "tx" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
            >
              Transactions
            </Link>
            <Link
              href={buildAddressTabHref(normalizedHex, "token", data.providedPrincipal)}
              scroll={false}
              className={`rounded-md border px-3 py-1 ${currentTab === "token" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
            >
              Token Transfers (ERC-20)
            </Link>
          </div>
        </CardHeader>
        <CardContent>
          {currentTab === "tx" ? (
            <>
              {data.history.length === 0 ? (
                <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
                  No indexed transactions for this address.
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Transaction Hash</TableHead>
                      <TableHead>Method</TableHead>
                      <TableHead>Block</TableHead>
                      <TableHead>Age</TableHead>
                      <TableHead>From</TableHead>
                      <TableHead>Direction</TableHead>
                      <TableHead>To</TableHead>
                      <TableHead>Amount</TableHead>
                      <TableHead>Txn Fee</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {data.history.map((item) => {
                      return (
                        <TableRow key={item.txHashHex}>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline" title={item.txHashHex}>
                              {shortPrefixHex(item.txHashHex)}
                            </Link>
                          </TableCell>
                          <TableCell>
                            <Badge variant="outline" title={item.txSelectorHex ?? undefined}>{item.methodLabel}</Badge>
                          </TableCell>
                          <TableCell>
                            <Link href={`/blocks/${item.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                              {item.blockNumber.toString()}
                            </Link>
                          </TableCell>
                          <TableCell>{formatAge(item.blockTimestamp)}</TableCell>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/address/${item.fromAddressHex}`} className="text-sky-700 hover:underline" title={item.fromAddressHex}>
                              {shortHex(item.fromAddressHex)}
                            </Link>
                          </TableCell>
                          <TableCell>
                            <Badge variant={item.direction === "in" ? "secondary" : "outline"}>{item.direction}</Badge>
                          </TableCell>
                          <TableCell className="font-mono text-xs">
                            {item.toAddressHex ? (
                              <Link href={`/address/${item.toAddressHex}`} className="text-sky-700 hover:underline" title={item.toAddressHex}>
                                {shortHex(item.toAddressHex)}
                              </Link>
                            ) : item.createdContractAddressHex ? (
                              <Link
                                href={`/address/${item.createdContractAddressHex}`}
                                className="text-sky-700 hover:underline"
                                title={item.createdContractAddressHex}
                              >
                                Contract Creation
                              </Link>
                            ) : (
                              "Contract Creation"
                            )}
                          </TableCell>
                          <TxValueFeeCells txHashHex={item.txHashHex} canisterId={canisterId} icHost={icHost} />
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              )}
              {data.nextCursor ? (
                <div className="mt-3 text-sm">
                  <Link
                    href={buildAddressCursorHref(normalizedHex, "tx", data.nextCursor, data.providedPrincipal)}
                    className="text-sky-700 hover:underline"
                  >
                    Older
                  </Link>
                </div>
              ) : null}
            </>
          ) : (
            <>
              {data.tokenTransfers.length === 0 ? (
                <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
                  No token transfers for this address.
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Transaction Hash</TableHead>
                      <TableHead>Method</TableHead>
                      <TableHead>Block</TableHead>
                      <TableHead>Age</TableHead>
                      <TableHead>From</TableHead>
                      <TableHead>To</TableHead>
                      <TableHead>Amount</TableHead>
                      <TableHead>Token</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {data.tokenTransfers.map((item) => {
                      return (
                        <TableRow key={`${item.txHashHex}:${item.logIndex}`}>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline" title={item.txHashHex}>
                              {shortPrefixHex(item.txHashHex)}
                            </Link>
                          </TableCell>
                          <TableCell>
                            <Badge variant="outline" title={item.txSelectorHex ?? undefined}>{item.methodLabel}</Badge>
                          </TableCell>
                          <TableCell>
                            <Link href={`/blocks/${item.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                              {item.blockNumber.toString()}
                            </Link>
                          </TableCell>
                          <TableCell>{formatAge(item.blockTimestamp)}</TableCell>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/address/${item.fromAddressHex}`} className="text-sky-700 hover:underline" title={item.fromAddressHex}>
                              {shortHex(item.fromAddressHex)}
                            </Link>
                          </TableCell>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/address/${item.toAddressHex}`} className="text-sky-700 hover:underline" title={item.toAddressHex}>
                              {shortHex(item.toAddressHex)}
                            </Link>
                          </TableCell>
                          <TableCell className="font-mono text-xs break-all">{item.amount.toString()}</TableCell>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/address/${item.tokenAddressHex}`} className="text-sky-700 hover:underline" title={item.tokenAddressHex}>
                              {shortHex(item.tokenAddressHex)}
                            </Link>
                          </TableCell>
                        </TableRow>
                      );
                    })}
                  </TableBody>
                </Table>
              )}
              {data.tokenNextCursor ? (
                <div className="mt-3 text-sm">
                  <Link
                    href={buildAddressCursorHref(normalizedHex, "token", data.tokenNextCursor, data.providedPrincipal)}
                    className="text-sky-700 hover:underline"
                  >
                    Older
                  </Link>
                </div>
              ) : null}
            </>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

async function loadSourceBundle(sourceBlobId: string): Promise<Record<string, string> | null> {
  const sourceBlob = await getVerifyBlobById(sourceBlobId);
  if (!sourceBlob) {
    return null;
  }
  return decodeSourceBundleFromGzip(sourceBlob.blob);
}

function shortPrefixHex(value: string, keep: number = 10): string {
  if (value.length <= keep) {
    return value;
  }
  return `${value.slice(0, keep)}...`;
}

function formatAge(rawTimestamp: bigint | null): string {
  if (rawTimestamp === null) {
    return "N/A";
  }
  const nowSec = BigInt(Math.floor(Date.now() / 1000));
  const tsSec = rawTimestamp > 10_000_000_000n ? rawTimestamp / 1000n : rawTimestamp;
  const delta = nowSec > tsSec ? nowSec - tsSec : 0n;
  if (delta < 60n) {
    return `${delta.toString()}s ago`;
  }
  if (delta < 3600n) {
    return `${(delta / 60n).toString()}m ago`;
  }
  if (delta < 86_400n) {
    return `${(delta / 3600n).toString()}h ago`;
  }
  return `${(delta / 86_400n).toString()}d ago`;
}

function buildAddressTabHref(addressHex: string, tab: AddressTab, principal: string | null): string {
  const query = new URLSearchParams();
  query.set("tab", tab);
  if (principal) {
    query.set("principal", principal);
  }
  return `/address/${addressHex}?${query.toString()}`;
}

function buildAddressCursorHref(
  addressHex: string,
  tab: AddressTab,
  cursor: string,
  principal: string | null
): string {
  const query = new URLSearchParams();
  query.set("tab", tab);
  if (tab === "tx") {
    query.set("cursor", cursor);
  } else {
    query.set("tokenCursor", cursor);
  }
  if (principal) {
    query.set("principal", principal);
  }
  return `/address/${addressHex}?${query.toString()}`;
}
