// どこで: viemスモーク / 何を: Gateway JSON-RPCの最小互換を実接続で確認 / なぜ: phase2要件のクライアント接続可否を担保するため

declare const process: {
  env: Record<string, string | undefined>;
  exit(code?: number): never;
  stderr: { write(chunk: string): void };
  stdout: { write(chunk: string): void };
};

async function main(): Promise<void> {
  const rpcUrl = process.env.EVM_RPC_URL ?? "http://127.0.0.1:8545";
  let createPublicClient: ((args: unknown) => unknown) | null = null;
  let http: ((url: string) => unknown) | null = null;

  try {
    const mod = await import("viem");
    createPublicClient = mod.createPublicClient as (args: unknown) => unknown;
    http = mod.http as (url: string) => unknown;
  } catch {
    process.stdout.write("[smoke:viem] SKIP: viem is not installed\n");
    return;
  }

  const chain = {
    id: 0,
    name: "kasane",
    network: "kasane",
    nativeCurrency: { name: "ICP", symbol: "ICP", decimals: 18 },
    rpcUrls: { default: { http: [rpcUrl] } },
  };
  const client = createPublicClient({ chain, transport: http(rpcUrl) }) as {
    getChainId: () => Promise<number>;
    getBlockNumber: () => Promise<bigint>;
    getBalance: (arg: { address: `0x${string}` }) => Promise<bigint>;
    getStorageAt: (arg: { address: `0x${string}`; slot: `0x${string}` }) => Promise<`0x${string}`>;
    call: (arg: { to: `0x${string}`; data?: `0x${string}` }) => Promise<{ data: `0x${string}` }>;
    estimateGas: (arg: { to: `0x${string}`; data?: `0x${string}` }) => Promise<bigint>;
    request: (arg: { method: string; params: unknown[] }) => Promise<unknown>;
  };

  const zero = "0x0000000000000000000000000000000000000000" as const;
  const slot0 = "0x0000000000000000000000000000000000000000000000000000000000000000" as const;

  const chainId = await client.getChainId();
  const blockNumber = await client.getBlockNumber();
  const balance = await client.getBalance({ address: zero });
  const storage = await client.getStorageAt({ address: zero, slot: slot0 });
  const callOut = await client.call({ to: zero, data: "0x" });
  const estimate = await client.estimateGas({ to: zero, data: "0x" });
  const revertData = await probeRevertData(client);

  process.stdout.write(
    `[smoke:viem] ok chainId=${chainId} block=${blockNumber} balance=${balance} storage=${storage} call=${callOut.data} estimate=${estimate} revertData=${revertData}\n`
  );
}

main().catch((err) => {
  const detail = err instanceof Error ? err.stack ?? err.message : String(err);
  process.stderr.write(`[smoke:viem] FAIL: ${detail}\n`);
  process.exit(1);
});

async function probeRevertData(client: {
  request: (arg: { method: string; params: unknown[] }) => Promise<unknown>;
}): Promise<string> {
  try {
    await client.request({
      method: "eth_call",
      params: [{ data: "0xfe" }, "latest"],
    });
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
