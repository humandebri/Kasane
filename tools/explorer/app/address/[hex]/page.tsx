// どこで: アドレス詳細ページ / 何を: 概要・Tx履歴・ERC-20 Transfer履歴をタブ表示 / なぜ: Etherscan風の調査導線を提供するため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../../components/ui/table";
import { TxHashLink } from "../../../components/tx-hash-link";
import { TxDirectionBadge } from "../../../components/tx-direction-badge";
import { TxValueFeeCells } from "../../../components/tx-value-fee-cells";
import { getAddressView } from "../../../lib/data";
import { formatIcpAmountFromWei } from "../../../lib/format";
import { isAddressHex, normalizeHex, shortHex } from "../../../lib/hex";

export const dynamic = "force-dynamic";

type AddressTab = "tx" | "internal" | "token" | "events" | "contract";

export default async function AddressPage({
  params,
  searchParams,
}: {
  params: Promise<{ hex: string }>;
  searchParams: Promise<{
    cursor?: string;
    internalCursor?: string;
    tokenCursor?: string;
    eventsCursor?: string;
    principal?: string;
    tab?: string;
  }>;
}) {
  const { hex } = await params;
  const { cursor, internalCursor, tokenCursor, eventsCursor, principal, tab } = await searchParams;
  if (!isAddressHex(hex)) {
    notFound();
  }
  const normalizedHex = normalizeHex(hex);
  const currentTab: AddressTab =
    tab === "internal" || tab === "token" || tab === "events" || tab === "contract" ? tab : "tx";
  const data = await getAddressView(
    normalizedHex,
    cursor,
    tokenCursor,
    eventsCursor,
    principal ?? null,
    internalCursor,
    { includeContractEvents: currentTab === "events" }
  );
  const canisterId = process.env.EVM_CANISTER_ID ?? null;
  const icHost = process.env.EXPLORER_IC_HOST ?? process.env.INDEXER_IC_HOST ?? "https://icp-api.io";
  const contractInfo = data.contractInfo;
  const contractRuntimeMatch = contractInfo?.runtimeMatch ?? null;
  const contractCreationMatch = contractInfo?.creationMatch ?? null;
  const contractVerifyMatchText =
    contractRuntimeMatch === null || contractCreationMatch === null
      ? "N/A"
      : `runtime=${contractRuntimeMatch ? "ok" : "ng"} / creation=${contractCreationMatch ? "ok" : "ng"}`;

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
              <Badge variant={contractInfo?.verified ? "secondary" : "outline"}>
                {contractInfo?.verified ? "Verified" : "Not Verified"}
              </Badge>
              {!contractInfo?.verified ? (
                <Link href={`/verify?address=${encodeURIComponent(normalizedHex)}`} className="text-sky-700 hover:underline">
                  Submit
                </Link>
              ) : null}
            </dd>
            {data.erc20Meta ? (
              <>
                <dt className="text-slate-500">Token Name</dt>
                <dd>{data.erc20Meta.name === "" ? "N/A" : data.erc20Meta.name}</dd>
                <dt className="text-slate-500">Symbol</dt>
                <dd className="font-mono">{data.erc20Meta.symbol}</dd>
                <dt className="text-slate-500">Decimals</dt>
                <dd>{data.erc20Meta.decimals.toString()}</dd>
                <dt className="text-slate-500">Total Supply</dt>
                <dd className="font-mono">{data.erc20Meta.totalSupplyFormatted}</dd>
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
              href={buildAddressTabHref(normalizedHex, "internal", data.providedPrincipal)}
              scroll={false}
              className={`rounded-md border px-3 py-1 ${currentTab === "internal" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
            >
              Internal Transactions
            </Link>
            <Link
              href={buildAddressTabHref(normalizedHex, "token", data.providedPrincipal)}
              scroll={false}
              className={`rounded-md border px-3 py-1 ${currentTab === "token" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
            >
              Token Transfers (ERC-20)
            </Link>
            <Link
              href={buildAddressTabHref(normalizedHex, "events", data.providedPrincipal)}
              scroll={false}
              className={`rounded-md border px-3 py-1 ${currentTab === "events" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
            >
              Contract Events
            </Link>
            <Link
              href={buildAddressTabHref(normalizedHex, "contract", data.providedPrincipal)}
              scroll={false}
              className={`rounded-md border px-3 py-1 ${currentTab === "contract" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
            >
              Contract
            </Link>
          </div>
        </CardHeader>
        <CardContent>
          {currentTab === "tx" ? (
            <>
              {data.history.length === 0 ? (
                <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
                  {data.isContract === true && data.tokenTransfers.length > 0
                    ? "No direct from/to transactions for this contract. Check Token Transfers for mint/burn activity."
                    : "No indexed transactions for this address."}
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
                            <TxHashLink txHashHex={item.txHashHex} receiptStatus={item.receiptStatus} title={item.txHashHex}>
                              {shortPrefixHex(item.txHashHex)}
                            </TxHashLink>
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
                            <TxDirectionBadge direction={item.direction} />
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
          ) : currentTab === "internal" ? (
            <>
              {data.internalTraceOverflowTxs.length > 0 ? (
                <div className="mb-3 rounded-md border border-amber-200 bg-amber-50 p-3 text-sm text-amber-900">
                  {data.internalTraceOverflowTxs.map((item) => (
                    <div key={item.txHashHex}>
                      {`Tx ${shortPrefixHex(item.txHashHex)} は internal trace 上限 1024 件で打ち切られています。表示中: ${item.capturedCount ?? "?"} / 全体: ${item.totalCount ?? "?"}`}
                    </div>
                  ))}
                </div>
              ) : null}
              {data.internalTraceFailedTxs.length > 0 ? (
                <div className="mb-3 rounded-md border border-rose-200 bg-rose-50 p-3 text-sm text-rose-900">
                  {data.internalTraceFailedTxs.map((item) => (
                    <div key={item.txHashHex}>
                      {`Tx ${shortPrefixHex(item.txHashHex)} の internal trace 保存に失敗しました。全体件数: ${item.totalCount ?? "?"}`}
                    </div>
                  ))}
                </div>
              ) : null}
              {data.internalTransactions.length === 0 ? (
                <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
                  No internal transactions for this address.
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Parent Tx Hash</TableHead>
                      <TableHead>Block</TableHead>
                      <TableHead>Age</TableHead>
                      <TableHead>Type</TableHead>
                      <TableHead>From</TableHead>
                      <TableHead>To</TableHead>
                      <TableHead>Value</TableHead>
                      <TableHead>Trace ID</TableHead>
                      <TableHead>Status</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {data.internalTransactions.map((item) => (
                      <TableRow key={`${item.txHashHex}:${item.traceId}`}>
                        <TableCell className="font-mono text-xs">
                          <TxHashLink txHashHex={item.txHashHex} receiptStatus={item.success ? 1 : 0} title={item.txHashHex}>
                            {shortPrefixHex(item.txHashHex)}
                          </TxHashLink>
                        </TableCell>
                        <TableCell>
                          <Link href={`/blocks/${item.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                            {item.blockNumber.toString()}
                          </Link>
                        </TableCell>
                        <TableCell>{formatAge(item.blockTimestamp)}</TableCell>
                        <TableCell>{item.actionType}</TableCell>
                        <TableCell className="font-mono text-xs">
                          <Link href={`/address/${item.fromAddressHex}`} className="text-sky-700 hover:underline" title={item.fromAddressHex}>
                            {shortHex(item.fromAddressHex)}
                          </Link>
                        </TableCell>
                        <TableCell className="font-mono text-xs">
                          {item.createdContractAddressHex ? (
                            <Link href={`/address/${item.createdContractAddressHex}`} className="text-sky-700 hover:underline" title={item.createdContractAddressHex}>
                              {shortHex(item.createdContractAddressHex)}
                            </Link>
                          ) : item.toAddressHex ? (
                            <Link href={`/address/${item.toAddressHex}`} className="text-sky-700 hover:underline" title={item.toAddressHex}>
                              {shortHex(item.toAddressHex)}
                            </Link>
                          ) : (
                            "-"
                          )}
                        </TableCell>
                        <TableCell className="font-mono text-xs">{item.valueText}</TableCell>
                        <TableCell className="font-mono text-xs">{item.traceId}</TableCell>
                        <TableCell className="text-xs">{item.success ? "success" : item.errorCode ?? "failed"}</TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
              {data.internalNextCursor ? (
                <div className="mt-3 text-sm">
                  <Link
                    href={buildAddressCursorHref(normalizedHex, "internal", data.internalNextCursor, data.providedPrincipal)}
                    className="text-sky-700 hover:underline"
                  >
                    Older
                  </Link>
                </div>
              ) : null}
            </>
          ) : currentTab === "token" ? (
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
                            <TxHashLink txHashHex={item.txHashHex} receiptStatus={item.receiptStatus} title={item.txHashHex}>
                              {shortPrefixHex(item.txHashHex)}
                            </TxHashLink>
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
                          <TableCell className="font-mono text-xs break-all">{item.amountText}</TableCell>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/address/${item.tokenAddressHex}`} className="text-sky-700 hover:underline" title={item.tokenAddressHex}>
                              {item.tokenLabel}
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
          ) : currentTab === "events" ? (
            <>
              {data.contractEventsUnavailable ? (
                <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
                  Contract events are unavailable because the canister logs query is not reachable.
                </div>
              ) : data.contractEvents.length === 0 ? (
                <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
                  No contract events for this address.
                </div>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>Block</TableHead>
                      <TableHead>Tx</TableHead>
                      <TableHead>Log</TableHead>
                      <TableHead>Event</TableHead>
                      <TableHead>topic0</TableHead>
                      <TableHead>Tx Hash</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {data.contractEvents.map((item) => (
                      <TableRow key={`${item.txHashHex}:${item.logIndex}`}>
                        <TableCell>
                          <Link href={`/blocks/${item.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                            {item.blockNumber.toString()}
                          </Link>
                        </TableCell>
                        <TableCell>{item.txIndex}</TableCell>
                        <TableCell>{item.logIndex}</TableCell>
                        <TableCell>{item.eventLabel}</TableCell>
                        <TableCell className="font-mono text-xs break-all">{item.topic0Hex ?? "-"}</TableCell>
                        <TableCell className="font-mono text-xs">
                          <TxHashLink txHashHex={item.txHashHex} receiptStatus={item.receiptStatus} title={item.txHashHex}>
                            {shortPrefixHex(item.txHashHex)}
                          </TxHashLink>
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
              {data.eventsNextCursor ? (
                <div className="mt-3 text-sm">
                  <Link
                    href={buildAddressCursorHref(normalizedHex, "events", data.eventsNextCursor, data.providedPrincipal)}
                    className="text-sky-700 hover:underline"
                  >
                    Older
                  </Link>
                </div>
              ) : null}
            </>
          ) : (
            <div className="space-y-4 text-sm">
              <dl className="grid grid-cols-1 gap-y-2 md:grid-cols-[190px_1fr]">
                <dt className="text-slate-500">Verification</dt>
                <dd className="flex items-center gap-2">
                  <Badge variant={contractInfo?.verified ? "secondary" : "outline"}>
                    {contractInfo?.verified ? "Verified" : "Not Verified"}
                  </Badge>
                  {!contractInfo?.verified ? (
                    <Link href={`/verify?address=${encodeURIComponent(normalizedHex)}`} className="text-sky-700 hover:underline">
                      Submit Verification
                    </Link>
                  ) : null}
                </dd>
                <dt className="text-slate-500">Creator</dt>
                <dd className="font-mono text-xs">
                  {contractInfo?.creatorAddressHex ? (
                    <Link href={`/address/${contractInfo.creatorAddressHex}`} className="text-sky-700 hover:underline">
                      {contractInfo.creatorAddressHex}
                    </Link>
                  ) : (
                    "N/A"
                  )}
                </dd>
                <dt className="text-slate-500">Creation Tx</dt>
                <dd className="font-mono text-xs">
                  {contractInfo?.creationTxHashHex ? (
                    <TxHashLink txHashHex={contractInfo.creationTxHashHex} receiptStatus={1}>
                      {contractInfo.creationTxHashHex}
                    </TxHashLink>
                  ) : (
                    "N/A"
                  )}
                </dd>
                <dt className="text-slate-500">Contract Name</dt>
                <dd>{contractInfo?.contractName ?? "N/A"}</dd>
                <dt className="text-slate-500">Compiler</dt>
                <dd className="font-mono text-xs">{contractInfo?.compilerVersion ?? "N/A"}</dd>
                <dt className="text-slate-500">Optimization</dt>
                <dd>
                  {contractInfo?.optimizerEnabled === null
                    ? "N/A"
                    : contractInfo?.optimizerEnabled
                      ? `enabled (runs=${contractInfo.optimizerRuns ?? 0})`
                      : "disabled"}
                </dd>
                <dt className="text-slate-500">Verify Match</dt>
                <dd>{contractVerifyMatchText}</dd>
                <dt className="text-slate-500">Source Files</dt>
                <dd>{contractInfo?.sourceBundle ? Object.keys(contractInfo.sourceBundle).length : 0}</dd>
                <dt className="text-slate-500">ABI</dt>
                <dd>{contractInfo?.abiJson ? (contractInfo.abiParseError ? "invalid json" : "available") : "N/A"}</dd>
              </dl>
              {contractInfo?.abiJson ? (
                <details className="rounded-md border border-slate-200 p-3">
                  <summary className="cursor-pointer text-xs">ABI JSON</summary>
                  <pre className="mt-2 max-h-64 overflow-auto rounded bg-slate-50 p-2 text-xs">{contractInfo.abiJson}</pre>
                </details>
              ) : null}
              {contractInfo?.sourceBundle ? (
                <details className="rounded-md border border-slate-200 p-3">
                  <summary className="cursor-pointer text-xs">Source Preview</summary>
                  <div className="mt-2 space-y-3">
                    {Object.entries(contractInfo.sourceBundle).map(([path, content]) => (
                      <div key={path}>
                        <div className="font-mono text-xs text-slate-600">{path}</div>
                        <pre className="max-h-48 overflow-auto rounded bg-slate-50 p-2 text-xs">{content}</pre>
                      </div>
                    ))}
                  </div>
                </details>
              ) : null}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
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
  } else if (tab === "internal") {
    query.set("internalCursor", cursor);
  } else if (tab === "token") {
    query.set("tokenCursor", cursor);
  } else {
    query.set("eventsCursor", cursor);
  }
  if (principal) {
    query.set("principal", principal);
  }
  return `/address/${addressHex}?${query.toString()}`;
}
