// where: gateway entrypoint / what: loads config and starts HTTP server / why: keep execution flow explicit

declare const process: {
  stderr: { write(chunk: string): void };
  exit(code?: number): never;
};

import { CONFIG } from "./config.js";
import { startServer } from "./server.js";

function main(): void {
  const server = startServer();
  process.stderr.write(
    `[rpc-gateway] listening on http://${CONFIG.host}:${CONFIG.port} canister=${CONFIG.canisterId} ic_host=${CONFIG.icHost}\n`
  );
  server.on("error", (err: unknown) => {
    const message = err instanceof Error ? err.stack ?? err.message : String(err);
    process.stderr.write(`[rpc-gateway] fatal: ${message}\n`);
    process.exit(1);
  });
}

main();
