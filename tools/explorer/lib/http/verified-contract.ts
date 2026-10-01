// どこで: 公開verify参照API / 何を: アドレスごとの検証結果を返す / なぜ: Explorer画面と外部連携で共通利用するため

import { getVerifiedContractByAddress, getVerifyBlobById } from "../db";
import { loadConfig } from "../config";
import { isAddressHex, normalizeHex } from "../hex";
import { decodeSourceBundleFromGzip } from "../verify/source_bundle";
import { parseChainId, parseVerifiedAbi } from "../verify/verified_contract_api";

export async function GET(
  request: Request,
  { params }: { params: Promise<{ address: string }> }
) {
  const cfg = loadConfig(process.env);
  const { address } = await params;
  if (!isAddressHex(address)) {
    return Response.json({ error: "invalid address" }, { status: 400 });
  }
  const chainIdRaw = new URL(request.url).searchParams.get("chainId");
  const chainId = parseChainId(chainIdRaw, cfg.verifyDefaultChainId);
  if (chainId === null) {
    return Response.json({ error: "invalid chainId" }, { status: 400 });
  }
  const found = await getVerifiedContractByAddress(normalizeHex(address), chainId);
  if (!found) {
    return Response.json({ isVerified: false });
  }
  const sourceBlob = await getVerifyBlobById(found.sourceBlobId);
  const sourceBundle = sourceBlob ? decodeSourceBundleFromGzip(sourceBlob.blob) : null;
  const parsedAbi = parseVerifiedAbi(found.abiJson);
  return Response.json({
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
