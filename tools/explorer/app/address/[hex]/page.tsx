// どこで: アドレス詳細ページ / 何を: 概要・Tx履歴・ERC-20 Transfer履歴をタブ表示 / なぜ: Etherscan風の調査導線を提供するため

import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "../../../components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "../../../components/ui/card";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "../../../components/ui/table";
import { getAddressView } from "../../../lib/data";
import { receiptStatusLabel } from "../../../lib/format";
import { isAddressHex, normalizeHex, shortHex } from "../../../lib/hex";

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
  const data = await getAddressView(normalizedHex, cursor, tokenCursor, principal ?? null);

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

          {data.observedPrincipals.length > 0 ? (
            <div className="rounded-lg border border-slate-200 bg-slate-50 p-3 text-sm">
              <div className="mb-1 font-medium">Observed Principals</div>
              <div className="flex flex-wrap gap-2">
                {data.observedPrincipals.map((p) => (
                  <Link
                    key={p}
                    href={`/principal/${encodeURIComponent(p)}`}
                    className="rounded-full border border-slate-300 bg-white px-3 py-1 font-mono text-xs text-sky-700 hover:underline"
                  >
                    {p}
                  </Link>
                ))}
              </div>
            </div>
          ) : null}

          <dl className="grid grid-cols-1 gap-y-2 text-sm md:grid-cols-[190px_1fr]">
            <dt className="text-slate-500">Balance (wei)</dt>
            <dd className="font-mono">{data.balance === null ? "N/A" : data.balance.toString()}</dd>
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
              className={`rounded-full border px-3 py-1 ${currentTab === "tx" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
            >
              Transactions
            </Link>
            <Link
              href={buildAddressTabHref(normalizedHex, "token", data.providedPrincipal)}
              className={`rounded-full border px-3 py-1 ${currentTab === "token" ? "border-sky-300 bg-sky-50 text-sky-700" : "border-slate-300 bg-white text-slate-700"}`}
            >
              Token Transfers
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
                      <TableHead>Tx Hash</TableHead>
                      <TableHead>Block</TableHead>
                      <TableHead>Index</TableHead>
                      <TableHead>Status</TableHead>
                      <TableHead>Direction</TableHead>
                      <TableHead>Counterparty</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {data.history.map((item) => {
                      const status = receiptStatusLabel(item.receiptStatus);
                      return (
                        <TableRow key={item.txHashHex}>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline" title={item.txHashHex}>
                              {shortHex(item.txHashHex)}
                            </Link>
                          </TableCell>
                          <TableCell>
                            <Link href={`/blocks/${item.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                              {item.blockNumber.toString()}
                            </Link>
                          </TableCell>
                          <TableCell>{item.txIndex}</TableCell>
                          <TableCell>
                            <Badge variant={status === "success" ? "secondary" : status === "failed" ? "default" : "outline"}>{status}</Badge>
                          </TableCell>
                          <TableCell>
                            <Badge variant={item.direction === "in" ? "secondary" : "outline"}>{item.direction}</Badge>
                          </TableCell>
                          <TableCell className="font-mono text-xs">
                            {item.counterpartyHex ? (
                              <Link href={`/address/${item.counterpartyHex}`} className="text-sky-700 hover:underline" title={item.counterpartyHex}>
                                {shortHex(item.counterpartyHex)}
                              </Link>
                            ) : (
                              "(contract creation)"
                            )}
                          </TableCell>
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
                      <TableHead>Tx Hash</TableHead>
                      <TableHead>Block</TableHead>
                      <TableHead>Log</TableHead>
                      <TableHead>Direction</TableHead>
                      <TableHead>Token</TableHead>
                      <TableHead>Counterparty</TableHead>
                      <TableHead>Amount</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {data.tokenTransfers.map((item) => {
                      const counterparty = item.direction === "in" ? item.fromAddressHex : item.toAddressHex;
                      return (
                        <TableRow key={`${item.txHashHex}:${item.logIndex}`}>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline" title={item.txHashHex}>
                              {shortHex(item.txHashHex)}
                            </Link>
                          </TableCell>
                          <TableCell>
                            <Link href={`/blocks/${item.blockNumber.toString()}`} className="text-sky-700 hover:underline">
                              {item.blockNumber.toString()}
                            </Link>
                          </TableCell>
                          <TableCell>{item.logIndex}</TableCell>
                          <TableCell>
                            <Badge variant={item.direction === "in" ? "secondary" : "outline"}>{item.direction}</Badge>
                          </TableCell>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/address/${item.tokenAddressHex}`} className="text-sky-700 hover:underline" title={item.tokenAddressHex}>
                              {shortHex(item.tokenAddressHex)}
                            </Link>
                          </TableCell>
                          <TableCell className="font-mono text-xs">
                            <Link href={`/address/${counterparty}`} className="text-sky-700 hover:underline" title={counterparty}>
                              {shortHex(counterparty)}
                            </Link>
                          </TableCell>
                          <TableCell className="font-mono text-xs break-all">{item.amount.toString()}</TableCell>
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

      <Card className="border-slate-200 bg-white shadow-sm">
        <CardHeader>
          <CardTitle>Failed Transactions (status=0)</CardTitle>
        </CardHeader>
        <CardContent>
          {data.failedHistory.length === 0 ? (
            <div className="rounded-md border bg-slate-50 p-3 text-sm text-muted-foreground">
              No failed transactions in this page.
            </div>
          ) : (
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Tx Hash</TableHead>
                  <TableHead>Block</TableHead>
                  <TableHead>Index</TableHead>
                  <TableHead>Direction</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {data.failedHistory.map((item) => (
                  <TableRow key={`failed:${item.txHashHex}`}>
                    <TableCell className="font-mono text-xs">
                      <Link href={`/tx/${item.txHashHex}`} className="text-sky-700 hover:underline" title={item.txHashHex}>
                        {shortHex(item.txHashHex)}
                      </Link>
                    </TableCell>
                    <TableCell>{item.blockNumber.toString()}</TableCell>
                    <TableCell>{item.txIndex}</TableCell>
                    <TableCell>
                      <Badge variant={item.direction === "in" ? "secondary" : "outline"}>{item.direction}</Badge>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
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
