// どこで: CLIエントリ / 何を: 設定ロードと起動 / なぜ: 実行を単純化するため

import { loadConfig } from "./config";
import { runWorker } from "./worker";

async function main(): Promise<void> {
  const config = loadConfig(process.env);
  await runWorker(config);
}

main().catch((err) => {
  const detail = err instanceof Error ? err.stack ?? err.message : String(err);
  process.stderr.write(`[indexer] fatal: ${detail}\n`);
  process.exit(1);
});
