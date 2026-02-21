// どこで: 公開verify参照API / 何を: アドレスごとの検証結果を返す / なぜ: Explorer画面と外部連携で共通利用するため

import { NextResponse, type NextRequest } from "next/server";
import { getVerifiedContractByAddress, getVerifyBlobById } from "../../../../../lib/db";
import { loadConfig } from "../../../../../lib/config";
import { isAddressHex, normalizeHex } from "../../../../../lib/hex";
import { decodeSourceBundleFromGzip } from "../../../../../lib/verify/source_bundle";
import { parseChainId, parseVerifiedAbi } from "../../../../../lib/verify/verified_contract_api";

export async function GET(
  request: NextRequest,
  { params }: { params: Promise<{ address: string }> }
) {
  const cfg = loadConfig(process.env);
  const { address } = await params;
  if (!isAddressHex(address)) {
    return NextResponse.json({ error: "invalid address" }, { status: 400 });
  }
  const chainIdRaw = request.nextUrl.searchParams.get("chainId");
  const chainId = parseChainId(chainIdRaw, cfg.verifyDefaultChainId);
  if (chainId === null) {
    return NextResponse.json({ error: "invalid chainId" }, { status: 400 });
  }
  const found = await getVerifiedContractByAddress(normalizeHex(address), chainId);
  if (!found) {
    return NextResponse.json({ isVerified: false });
  }
  const sourceBlob = await getVerifyBlobById(found.sourceBlobId);
  const sourceBundle = sourceBlob ? decodeSourceBundleFromGzip(sourceBlob.blob) : null;
  const parsedAbi = parseVerifiedAbi(found.abiJson);
  return NextResponse.json({
    isVerified: true,
    contractName: found.contractName,
    compiler: found.compilerVersion,
    optimization: {
      enabled: found.optimizerEnabled,
      runs: found.optimizerRuns,
      evmVersion: found.evmVersion,
    },
    abi: parsedAbi.abi,
    abiParseError: parsedAbi.abiParseError,
    sourceRefs: { sourceBlobId: found.sourceBlobId, metadataBlobId: found.metadataBlobId },
    sourceBundle,
    verifiedAt: found.publishedAt.toString(),
    creationMatch: found.creationMatch,
    runtimeMatch: found.runtimeMatch,
  });
}
