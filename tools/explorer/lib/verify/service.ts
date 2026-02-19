// どこで: verify実行サービス / 何を: コンパイル・on-chain照合・補助照合を統合 / なぜ: API/ワーカーから同一ロジックを再利用するため

import { getRpcCode } from "../rpc";
import { parseAddressHex, toHexLower } from "../hex";
import { getDeployTxInputByContractAddress } from "../db";
import { compileVerifyInput, isRuntimeMatch } from "./compile";
import { querySourcifyStatus } from "./sourcify";
import type { SourcifyStatus } from "./sourcify";
import type { VerifyExecutionResult, VerifyJobErrorCode, VerifySubmitInput } from "./types";

type VerifyDeps = {
  getRuntimeCode: (address: Uint8Array) => Promise<Uint8Array>;
  getDeployInput: (address: Uint8Array) => Promise<{ found: boolean; txInput: Uint8Array | null }>;
  checkSourcify: (chainId: number, contractAddress: string) => Promise<SourcifyStatus>;
};

const defaultDeps: VerifyDeps = {
  getRuntimeCode: getRpcCode,
  getDeployInput: getDeployTxInputByContractAddress,
  checkSourcify: querySourcifyStatus,
};

let depsForTest: VerifyDeps | null = null;

export async function executeVerifyJob(input: VerifySubmitInput): Promise<VerifyExecutionResult> {
  const deps = depsForTest ?? defaultDeps;
  let compiled;
  try {
    compiled = await compileVerifyInput(input);
  } catch (err) {
    throw mapError(err);
  }

  let onChainRuntimeHex: string;
  try {
    const runtime = await deps.getRuntimeCode(parseAddressHex(input.contractAddress));
    onChainRuntimeHex = toHexLower(runtime);
  } catch {
    throw mkVerifyError("rpc_unavailable", "failed to fetch runtime bytecode");
  }

  const runtimeMatch = isRuntimeMatch(compiled.runtimeBytecodeHex, onChainRuntimeHex);
  if (!runtimeMatch) {
    throw mkVerifyError("runtime_mismatch", "compiled runtime does not match on-chain code");
  }
  const deployInfo = await deps.getDeployInput(parseAddressHex(input.contractAddress));
  if (!deployInfo.found) {
    throw mkVerifyError("deploy_tx_not_found", "deploy tx is not indexed for target contract");
  }
  if (!deployInfo.txInput) {
    throw mkVerifyError("creation_input_missing", "deploy tx input is missing");
  }
  const creationCandidate = `${compiled.creationBytecodeHex}${input.constructorArgsHex.replace(/^0x/, "").toLowerCase()}`;
  const txInput = Buffer.from(deployInfo.txInput).toString("hex").toLowerCase();
  const creationMatch = txInput === creationCandidate;
  if (!creationMatch) {
    throw mkVerifyError("creation_mismatch", "compiled creation bytecode + constructor args mismatch");
  }

  let sourcifyStatus: SourcifyStatus = "not_found";
  try {
    sourcifyStatus = await deps.checkSourcify(input.chainId, input.contractAddress);
  } catch {
    sourcifyStatus = "error";
  }

  return {
    creationMatch: true,
    runtimeMatch,
    abiJson: compiled.abiJson,
    metadataJson: compiled.metadataJson,
    sourcifyStatus,
  };
}

export type VerifyServiceError = {
  code: VerifyJobErrorCode;
  message: string;
};

export function isVerifyServiceError(value: unknown): value is VerifyServiceError {
  if (!isRecord(value)) {
    return false;
  }
  return typeof value.code === "string" && typeof value.message === "string";
}

export function mkVerifyError(code: VerifyJobErrorCode, message: string): VerifyServiceError {
  return { code, message };
}

function mapError(err: unknown): VerifyServiceError {
  if (isVerifyServiceError(err)) {
    return err;
  }
  const message = err instanceof Error ? err.message : String(err);
  if (message.startsWith("compiler_unavailable:")) {
    return mkVerifyError("compiler_unavailable", message.replace("compiler_unavailable:", "").trim());
  }
  if (message.startsWith("compile_error:")) {
    return mkVerifyError("compile_error", message.replace("compile_error:", "").trim());
  }
  if (message.startsWith("compile_timeout:")) {
    return mkVerifyError("compile_timeout", message.replace("compile_timeout:", "").trim());
  }
  if (message.startsWith("compile_resource_exceeded:")) {
    return mkVerifyError("compile_resource_exceeded", message.replace("compile_resource_exceeded:", "").trim());
  }
  if (message.startsWith("compile_process_failed:")) {
    return mkVerifyError("compile_process_failed", message.replace("compile_process_failed:", "").trim());
  }
  if (message.startsWith("contract_not_found:")) {
    return mkVerifyError("contract_not_found", message.replace("contract_not_found:", "").trim());
  }
  return mkVerifyError("internal_error", message);
}

export const verifyServiceTestHooks = {
  setDepsForTest(deps: VerifyDeps): void {
    depsForTest = deps;
  },
  resetDepsForTest(): void {
    depsForTest = null;
  },
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
