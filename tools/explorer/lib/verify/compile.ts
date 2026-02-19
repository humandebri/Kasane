// どこで: verifyコンパイル層 / 何を: ローカルsolcバイナリを安全制約付きで実行 / なぜ: 本番運用でDoS耐性と再現性を担保するため

import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { loadConfig } from "../config";
import type { VerifySubmitInput } from "./types";

type SolcError = {
  severity?: string;
  formattedMessage?: string;
  message?: string;
};

type SolcContractArtifact = {
  abi?: unknown;
  evm?: {
    bytecode?: { object?: string };
    deployedBytecode?: { object?: string };
  };
  metadata?: string;
};

type SolcOutput = {
  errors?: SolcError[];
  contracts?: Record<string, Record<string, SolcContractArtifact>>;
};

export type CompiledVerifyArtifact = {
  abiJson: string;
  creationBytecodeHex: string;
  runtimeBytecodeHex: string;
  metadataJson: string;
  compilerVersion: string;
};

const MAX_SOURCE_FILES = 100;
const MAX_SOURCE_FILE_BYTES = 512 * 1024;
const MAX_SOURCE_TOTAL_BYTES = 5 * 1024 * 1024;
const MAX_OPTIMIZER_RUNS = 1_000_000;
const MAX_STDIO_BYTES = 10 * 1024 * 1024;
const KILL_GRACE_MS = 1500;

export async function ensureSolcBinaryAvailable(version: string): Promise<void> {
  const binary = `solc-${version}`;
  await new Promise<void>((resolve, reject) => {
    const child = spawn(binary, ["--version"], { stdio: ["ignore", "pipe", "pipe"] });
    let stderr = "";
    child.stderr.on("data", (chunk: Buffer) => {
      stderr += chunk.toString("utf8");
    });
    child.on("error", (err) => {
      reject(new Error(`compiler_unavailable:${binary} ${err.message}`));
    });
    child.on("close", (code) => {
      if (code !== 0) {
        reject(new Error(`compiler_unavailable:${binary} exit=${code} ${stderr.trim()}`));
        return;
      }
      resolve();
    });
  });
}

export async function compileVerifyInput(input: VerifySubmitInput): Promise<CompiledVerifyArtifact> {
  validateCompileInput(input);

  const solcBinary = `solc-${input.compilerVersion}`;
  const tempDir = await mkdtemp(join(tmpdir(), "verify-solc-"));
  try {
    const standardInput = {
      language: "Solidity",
      sources: Object.fromEntries(Object.entries(input.sourceBundle).map(([file, content]) => [file, { content }])),
      settings: {
        optimizer: {
          enabled: input.optimizerEnabled,
          runs: input.optimizerRuns,
        },
        evmVersion: input.evmVersion ?? undefined,
        outputSelection: {
          "*": {
            "*": ["abi", "metadata", "evm.bytecode.object", "evm.deployedBytecode.object"],
          },
        },
      },
    };
    const outText = await runSolcProcess(solcBinary, JSON.stringify(standardInput), tempDir);
    const output = parseSolcOutput(outText);
    const errorMessages = (output.errors ?? [])
      .filter((err) => err.severity === "error")
      .map((err) => err.formattedMessage ?? err.message ?? "unknown error");
    if (errorMessages.length > 0) {
      throw new Error(`compile_error:${errorMessages.join("\n")}`);
    }

    const artifact = findContractArtifact(output, input.contractName);
    const runtimeBytecodeHex = normalizeHexBytes(artifact.evm?.deployedBytecode?.object ?? "");
    const creationBytecodeHex = normalizeHexBytes(artifact.evm?.bytecode?.object ?? "");
    if (!runtimeBytecodeHex || !creationBytecodeHex) {
      throw new Error("contract_not_found:bytecode is empty");
    }
    return {
      abiJson: JSON.stringify(artifact.abi ?? []),
      creationBytecodeHex,
      runtimeBytecodeHex,
      metadataJson: artifact.metadata ?? "{}",
      compilerVersion: input.compilerVersion,
    };
  } finally {
    await rm(tempDir, { recursive: true, force: true });
  }
}

function validateCompileInput(input: VerifySubmitInput): void {
  if (input.optimizerRuns > MAX_OPTIMIZER_RUNS) {
    throw new Error("compile_resource_exceeded:optimizerRuns too high");
  }
  if (!isSafeContractName(input.contractName)) {
    throw new Error("invalid_input:contractName has forbidden characters");
  }
  const files = Object.entries(input.sourceBundle);
  if (files.length === 0 || files.length > MAX_SOURCE_FILES) {
    throw new Error("compile_resource_exceeded:source file count exceeded");
  }
  let totalBytes = 0;
  for (const [filePath, content] of files) {
    if (!isSafeSourcePath(filePath)) {
      throw new Error(`invalid_input:invalid source path ${filePath}`);
    }
    const size = Buffer.byteLength(content, "utf8");
    if (size > MAX_SOURCE_FILE_BYTES) {
      throw new Error(`compile_resource_exceeded:file too large ${filePath}`);
    }
    totalBytes += size;
    if (totalBytes > MAX_SOURCE_TOTAL_BYTES) {
      throw new Error("compile_resource_exceeded:source bundle too large");
    }
  }
}

function isSafeSourcePath(filePath: string): boolean {
  if (!filePath || filePath.length > 240) {
    return false;
  }
  if (filePath.includes("..") || filePath.startsWith("/") || filePath.startsWith("\\")) {
    return false;
  }
  return /^[a-zA-Z0-9_./\-]+$/.test(filePath);
}

function isSafeContractName(contractName: string): boolean {
  if (!contractName || contractName.length > 240) {
    return false;
  }
  return /^[a-zA-Z0-9_./:\-]+$/.test(contractName);
}

async function runSolcProcess(binary: string, stdinText: string, cwd: string): Promise<string> {
  const cfg = loadConfig(process.env);
  const timeoutMs = cfg.verifyJobTimeoutMs;

  return await new Promise<string>((resolve, reject) => {
    const child = spawn(binary, ["--standard-json"], {
      cwd,
      detached: true,
      stdio: ["pipe", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    let stdoutBytes = 0;
    let stderrBytes = 0;
    let done = false;

    const finish = (err: Error | null, out?: string): void => {
      if (done) {
        return;
      }
      done = true;
      clearTimeout(timer);
      if (err) {
        reject(err);
        return;
      }
      resolve(out ?? "");
    };

    const timer = setTimeout(() => {
      terminateChildGroup(child.pid ?? null);
      setTimeout(() => {
        killChildGroup(child.pid ?? null);
      }, KILL_GRACE_MS);
      finish(new Error("compile_timeout:solc timed out"));
    }, timeoutMs);

    child.on("error", (err) => {
      finish(new Error(`compile_process_failed:${err.message}`));
    });

    child.stdout.on("data", (chunk: Buffer) => {
      stdoutBytes += chunk.length;
      if (stdoutBytes > MAX_STDIO_BYTES) {
        terminateChildGroup(child.pid ?? null);
        setTimeout(() => {
          killChildGroup(child.pid ?? null);
        }, KILL_GRACE_MS);
        finish(new Error("compile_resource_exceeded:stdout too large"));
        return;
      }
      stdout += chunk.toString("utf8");
    });

    child.stderr.on("data", (chunk: Buffer) => {
      stderrBytes += chunk.length;
      if (stderrBytes > MAX_STDIO_BYTES) {
        terminateChildGroup(child.pid ?? null);
        setTimeout(() => {
          killChildGroup(child.pid ?? null);
        }, KILL_GRACE_MS);
        finish(new Error("compile_resource_exceeded:stderr too large"));
        return;
      }
      stderr += chunk.toString("utf8");
    });

    child.on("close", (code) => {
      if (done) {
        return;
      }
      if (code !== 0) {
        finish(new Error(`compile_process_failed:exit=${code} ${stderr.trim()}`));
        return;
      }
      finish(null, stdout);
    });

    child.stdin.write(stdinText);
    child.stdin.end();
  });
}

function terminateChildGroup(pid: number | null): void {
  if (!pid) {
    return;
  }
  try {
    process.kill(-pid, "SIGTERM");
  } catch {
    // no-op
  }
}

function killChildGroup(pid: number | null): void {
  if (!pid) {
    return;
  }
  try {
    process.kill(-pid, "SIGKILL");
  } catch {
    // no-op
  }
}

export function isRuntimeMatch(compiledRuntimeHex: string, onChainRuntimeHex: string): boolean {
  const compiledRaw = hexToBytes(normalizeHexBytes(compiledRuntimeHex));
  const onChainRaw = hexToBytes(normalizeHexBytes(onChainRuntimeHex));
  if (compiledRaw.length === 0 || onChainRaw.length === 0) {
    return false;
  }
  if (bytesEqual(compiledRaw, onChainRaw)) {
    return true;
  }
  return bytesEqual(stripSolidityMetadata(compiledRaw), stripSolidityMetadata(onChainRaw));
}

function findContractArtifact(output: SolcOutput, requestedName: string): SolcContractArtifact {
  const contracts = output.contracts ?? {};
  const [requestedFile, requestedContract] = splitContractSelector(requestedName);
  if (requestedFile && requestedContract) {
    const fileContracts = contracts[requestedFile];
    const artifact = fileContracts?.[requestedContract];
    if (!artifact) {
      throw new Error(`contract_not_found:${requestedName}`);
    }
    return artifact;
  }
  const hits: SolcContractArtifact[] = [];
  for (const fileContracts of Object.values(contracts)) {
    const artifact = fileContracts[requestedName];
    if (artifact) {
      hits.push(artifact);
    }
  }
  if (hits.length !== 1 || !hits[0]) {
    throw new Error(`contract_not_found:${requestedName}`);
  }
  return hits[0];
}

function splitContractSelector(input: string): [string, string] | [null, null] {
  const sep = input.indexOf(":");
  if (sep <= 0 || sep >= input.length - 1) {
    return [null, null];
  }
  return [input.slice(0, sep), input.slice(sep + 1)];
}

function normalizeHexBytes(value: string): string {
  return value.trim().toLowerCase().replace(/^0x/, "");
}

function stripSolidityMetadata(code: Uint8Array): Uint8Array {
  if (code.length < 2) {
    return code;
  }
  const secondLast = code.at(-2);
  const last = code.at(-1);
  if (secondLast === undefined || last === undefined) {
    return code;
  }
  const metadataLen = (secondLast << 8) | last;
  const metadataTotal = metadataLen + 2;
  if (metadataTotal <= 2 || metadataTotal >= code.length) {
    return code;
  }
  return code.slice(0, code.length - metadataTotal);
}

function hexToBytes(hexWithoutPrefix: string): Uint8Array {
  if (!hexWithoutPrefix || hexWithoutPrefix.length % 2 !== 0 || !/^[0-9a-f]+$/.test(hexWithoutPrefix)) {
    return new Uint8Array();
  }
  return Uint8Array.from(Buffer.from(hexWithoutPrefix, "hex"));
}

function bytesEqual(left: Uint8Array, right: Uint8Array): boolean {
  if (left.length !== right.length) {
    return false;
  }
  for (let i = 0; i < left.length; i += 1) {
    if (left[i] !== right[i]) {
      return false;
    }
  }
  return true;
}

function parseSolcOutput(outputText: string): SolcOutput {
  const parsed = JSON.parse(outputText);
  if (!isRecord(parsed)) {
    return {};
  }
  const out: SolcOutput = {};
  const errors = parsed.errors;
  if (Array.isArray(errors)) {
    out.errors = [];
    for (const item of errors) {
      if (!isRecord(item)) {
        continue;
      }
      out.errors.push({
        severity: typeof item.severity === "string" ? item.severity : undefined,
        formattedMessage: typeof item.formattedMessage === "string" ? item.formattedMessage : undefined,
        message: typeof item.message === "string" ? item.message : undefined,
      });
    }
  }
  if (isRecord(parsed.contracts)) {
    out.contracts = {};
    for (const [file, contractMap] of Object.entries(parsed.contracts)) {
      if (!isRecord(contractMap)) {
        continue;
      }
      const typedContractMap: Record<string, SolcContractArtifact> = {};
      for (const [contractName, artifactRaw] of Object.entries(contractMap)) {
        if (!isRecord(artifactRaw)) {
          continue;
        }
        const artifact: SolcContractArtifact = {};
        if (artifactRaw.abi !== undefined) {
          artifact.abi = artifactRaw.abi;
        }
        if (isRecord(artifactRaw.evm)) {
          artifact.evm = {};
          if (isRecord(artifactRaw.evm.bytecode)) {
            artifact.evm.bytecode = {
              object: typeof artifactRaw.evm.bytecode.object === "string" ? artifactRaw.evm.bytecode.object : undefined,
            };
          }
          if (isRecord(artifactRaw.evm.deployedBytecode)) {
            artifact.evm.deployedBytecode = {
              object:
                typeof artifactRaw.evm.deployedBytecode.object === "string"
                  ? artifactRaw.evm.deployedBytecode.object
                  : undefined,
            };
          }
        }
        if (typeof artifactRaw.metadata === "string") {
          artifact.metadata = artifactRaw.metadata;
        }
        typedContractMap[contractName] = artifact;
      }
      out.contracts[file] = typedContractMap;
    }
  }
  return out;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}
