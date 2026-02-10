// どこで: ethersスモーク / 何を: Gateway JSON-RPCの最小互換を実接続で確認 / なぜ: phase2要件のクライアント接続可否を担保するため

declare const process: {
  env: Record<string, string | undefined>;
  exit(code?: number): never;
  stderr: { write(chunk: string): void };
  stdout: { write(chunk: string): void };
};

async function main(): Promise<void> {
  const rpcUrl = process.env.EVM_RPC_URL ?? "http://127.0.0.1:8545";
  let JsonRpcProvider: (new (url: string) => {
    getNetwork: () => Promise<{ chainId: bigint }>;
    getBlockNumber: () => Promise<number>;
    getBalance: (address: string) => Promise<bigint>;
    getStorage: (address: string, position: bigint) => Promise<string>;
    call: (tx: { to: string; data: string }) => Promise<string>;
    estimateGas: (tx: { to: string; data: string }) => Promise<bigint>;
    send: (method: string, params: unknown[]) => Promise<unknown>;
  }) | null = null;

  try {
    const mod = await import("ethers");
    JsonRpcProvider = mod.JsonRpcProvider as typeof JsonRpcProvider;
  } catch {
    process.stdout.write("[smoke:ethers] SKIP: ethers is not installed\n");
    return;
  }

  const provider = new JsonRpcProvider(rpcUrl);
  const zero = "0x0000000000000000000000000000000000000000";

  const network = await provider.getNetwork();
  const blockNumber = await provider.getBlockNumber();
  const balance = await provider.getBalance(zero);
  const storage = await provider.getStorage(zero, 0n);
  const callOut = await provider.call({ to: zero, data: "0x" });
  const estimate = await provider.estimateGas({ to: zero, data: "0x" });
  const revertData = await probeRevertData(provider);

  process.stdout.write(
    `[smoke:ethers] ok chainId=${network.chainId} block=${blockNumber} balance=${balance} storage=${storage} call=${callOut} estimate=${estimate} revertData=${revertData}\n`
  );
}

main().catch((err) => {
  const detail = err instanceof Error ? err.stack ?? err.message : String(err);
  process.stderr.write(`[smoke:ethers] FAIL: ${detail}\n`);
  process.exit(1);
});

async function probeRevertData(provider: {
  send: (method: string, params: unknown[]) => Promise<unknown>;
}): Promise<string> {
  try {
    await provider.send("eth_call", [{ data: "0xfe" }, "latest"]);
    throw new Error("revert probe unexpectedly succeeded");
  } catch (err) {
    const data = extractErrorData(err);
    if (typeof data === "string" && data.startsWith("0x")) {
      return data;
    }
    throw new Error(`revert probe missing hex error.data: ${stringifyError(err)}`);
  }
}

function extractErrorData(err: unknown): unknown {
  if (!isRecord(err)) {
    return undefined;
  }
  if ("data" in err) {
    return err.data;
  }
  if ("error" in err && isRecord(err.error) && "data" in err.error) {
    return err.error.data;
  }
  if ("info" in err && isRecord(err.info) && "error" in err.info && isRecord(err.info.error) && "data" in err.info.error) {
    return err.info.error.data;
  }
  if ("cause" in err) {
    return extractErrorData(err.cause);
  }
  return undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function stringifyError(err: unknown): string {
  if (err instanceof Error) {
    return err.stack ?? err.message;
  }
  try {
    return JSON.stringify(err);
  } catch {
    return String(err);
  }
}
