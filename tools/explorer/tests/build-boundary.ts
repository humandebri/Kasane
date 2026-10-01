import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

async function audit(directory: string): Promise<void> {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) { await audit(path); continue; }
    if (!entry.name.endsWith(".js")) continue;
    const content = await readFile(path, "utf8");
    for (const marker of ["EXPLORER_DATABASE_URL", "EXPLORER_VERIFY_AUTH_HMAC_KEYS", "consumeVerifyReplayJti", "SELECT t.tx_hash", "Invalid PublicKey length", "test-build-secret-do-not-expose"]) {
      assert.ok(!content.includes(marker), `server-only code/config in ${entry.name}`);
    }
  }
}
await audit(".output/public");
console.log("client bundle boundary passed");
