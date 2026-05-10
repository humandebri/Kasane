// どこで: wrapper-vite local smoke helper / 何を: Juno emulator 前提の確認項目を表示する / なぜ: 手動スモークの入口を固定して見落としを減らすため

import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const cwd = process.cwd();
const envPath = resolve(cwd, ".env.local");
const requiredKeys = [
  "VITE_IC_HOST",
  "VITE_KASANE_EVM_CANISTER_ID",
  "VITE_EVM_WRAP_FACTORY",
  "VITE_JUNO_SATELLITE_ID",
  "JUNO_DEV_SATELLITE_ID",
];

function readEnvMap() {
  if (!existsSync(envPath)) {
    throw new Error(`missing_env_file:${envPath}`);
  }
  const text = readFileSync(envPath, "utf8");
  const env = new Map();
  for (const rawLine of text.split(/\r?\n/u)) {
    const line = rawLine.trim();
    if (line === "" || line.startsWith("#")) {
      continue;
    }
    const separatorIndex = line.indexOf("=");
    if (separatorIndex <= 0) {
      continue;
    }
    const key = line.slice(0, separatorIndex).trim();
    const value = line.slice(separatorIndex + 1).trim();
    env.set(key, value);
  }
  return env;
}

try {
  const env = readEnvMap();
  const missing = requiredKeys.filter((key) => {
    const value = env.get(key);
    return value === undefined || value === "";
  });

  console.log("wrapper-vite local smoke checklist");
  console.log("");
  console.log(`.env.local: ${envPath}`);
  if (missing.length > 0) {
    console.log(`missing env keys: ${missing.join(", ")}`);
    process.exitCode = 1;
  } else {
    console.log("required env keys: ok");
  }
  console.log("");
  console.log("manual smoke steps:");
  console.log("1. npm run juno:emulator:start");
  console.log("2. Open http://localhost:5866 and confirm the local Satellite exists.");
  console.log("3. npm run juno:functions:build");
  console.log("4. npm run dev");
  console.log("5. Confirm 'Connect Oisy to view request history.' appears on /history while disconnected.");
  console.log("6. Connect Oisy and confirm the wallet modal shows the signer principal.");
  console.log("7. Confirm Recent Requests shows the new request.");
  console.log("8. Reload and confirm the same principal sees the same request.");
  console.log("9. Open /requests/:requestId and confirm the status modal reopens.");
  console.log("10. In emulator Console, inspect recent_requests collection.");
} catch (error) {
  const message = error instanceof Error ? error.message : "local_smoke_failed";
  if (message.startsWith("missing_env_file:")) {
    console.error(message);
    console.error("run: cp .env.example .env.local");
  } else {
    console.error(message);
  }
  process.exitCode = 1;
}
