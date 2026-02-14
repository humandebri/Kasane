// どこで: Gatewayスモーク監視 / 何を: receipt.status をポーリングして成否判定 / なぜ: 送信成功と実行成功を分離して監視するため
type JsonRpcSuccess<T> = { jsonrpc: "2.0"; id: number; result: T };
type JsonRpcError = { jsonrpc: "2.0"; id: number; error: { code: number; message: string; data?: unknown } };
type JsonRpcResponse<T> = JsonRpcSuccess<T> | JsonRpcError;
type Receipt = { status?: string; blockNumber?: string; gasUsed?: string };

const rpcUrl = process.env.EVM_RPC_URL ?? "http://127.0.0.1:8545";
const txHash = process.argv[2];
const maxWaitSec = Number.parseInt(process.argv[3] ?? "120", 10);
const intervalMs = Number.parseInt(process.argv[4] ?? "1500", 10);

if (!txHash || !/^0x[0-9a-fA-F]{64}$/.test(txHash)) {
  console.error("usage: tsx smoke/receipt_watch.ts <txHash> [maxWaitSec=120] [intervalMs=1500]");
  process.exit(2);
}
if (!Number.isFinite(maxWaitSec) || maxWaitSec <= 0) {
  console.error("maxWaitSec must be a positive integer");
  process.exit(2);
}
if (!Number.isFinite(intervalMs) || intervalMs <= 0) {
  console.error("intervalMs must be a positive integer");
  process.exit(2);
}

async function main(): Promise<void> {
  const deadline = Date.now() + maxWaitSec * 1000;
  for (;;) {
    const receipt = await getReceipt(txHash);
    if (receipt !== null) {
      const status = typeof receipt.status === "string" ? receipt.status.toLowerCase() : "0x0";
      const summary = {
        txHash,
        status,
        blockNumber: receipt.blockNumber ?? null,
        gasUsed: receipt.gasUsed ?? null,
      };
      console.log(JSON.stringify(summary));
      if (status !== "0x1") {
        throw new Error(`receipt indicates execution failure: status=${status}`);
      }
      return;
    }
    if (Date.now() >= deadline) {
      throw new Error(`timeout waiting receipt: txHash=${txHash} waited=${maxWaitSec}s`);
    }
    await sleep(intervalMs);
  }
}

async function getReceipt(hash: string): Promise<Receipt | null> {
  const payload = {
    jsonrpc: "2.0",
    id: 1,
    method: "eth_getTransactionReceipt",
    params: [hash],
  };
  const response = await fetch(rpcUrl, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    throw new Error(`rpc http error: status=${response.status}`);
  }
  const json = (await response.json()) as JsonRpcResponse<Receipt | null>;
  if ("error" in json) {
    throw new Error(`rpc error: code=${json.error.code} message=${json.error.message}`);
  }
  return json.result;
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(message);
  process.exit(1);
});
