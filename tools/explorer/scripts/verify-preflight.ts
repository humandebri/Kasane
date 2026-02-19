// どこで: verify運用前チェック / 何を: allowlistのsolcバイナリ可用性を検証 / なぜ: 本番起動直後のcompiler_unavailable多発を防ぐため

import { loadConfig } from "../lib/config";
import { ensureSolcBinaryAvailable } from "../lib/verify/compile";

async function main(): Promise<void> {
  const cfg = loadConfig(process.env);
  if (!cfg.verifyEnabled) {
    console.log("verify disabled: skip preflight");
    return;
  }
  if (cfg.verifyAllowedCompilerVersions.length === 0) {
    throw new Error("EXPLORER_VERIFY_ALLOWED_COMPILER_VERSIONS is required");
  }
  for (const version of cfg.verifyAllowedCompilerVersions) {
    await ensureSolcBinaryAvailable(version);
    console.log(`ok: solc-${version}`);
  }
  console.log("verify preflight passed");
}

main().catch((err) => {
  console.error("verify preflight failed", err);
  process.exitCode = 1;
});
