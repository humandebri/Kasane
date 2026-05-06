// どこで: wrapper-vite local Juno smoke helper / 何を: wrapper-vite の env 前提とローカル検証手順をまとめる / なぜ: Juno emulator 側から現行 frontend の smoke を案内するため

import { existsSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const wrapperViteDir = resolve(__dirname, "..");
const wrapperViteEnvPath = resolve(wrapperViteDir, ".env.local");

function readEnvMap(envPath) {
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

function requireEnvKeys(env, keys, label) {
  const missing = keys.filter((key) => {
    const value = env.get(key);
    return value === undefined || value === "";
  });
  if (missing.length > 0) {
    throw new Error(`${label}.missing_env:${missing.join(",")}`);
  }
}

try {
  const wrapperViteEnv = readEnvMap(wrapperViteEnvPath);

  requireEnvKeys(wrapperViteEnv, [
    "VITE_JUNO_SATELLITE_ID",
    "JUNO_DEV_SATELLITE_ID",
  ], "wrapper-vite");

  const wrapperViteSatelliteId = wrapperViteEnv.get("VITE_JUNO_SATELLITE_ID");
  const wrapperViteDevSatelliteId = wrapperViteEnv.get("JUNO_DEV_SATELLITE_ID");
  if (wrapperViteSatelliteId !== wrapperViteDevSatelliteId) {
    throw new Error("satellite_id_mismatch:wrapper-vite env must use the same local satellite id");
  }

  console.log("wrapper-vite local Juno smoke checklist");
  console.log("");
  console.log(`wrapper-vite .env.local: ${wrapperViteEnvPath}`);
  console.log(`local satellite id: ${wrapperViteSatelliteId}`);
  console.log("");
  console.log("env consistency: ok");
  console.log("");
  console.log("manual steps:");
  console.log(`1. cd ${wrapperViteDir} && npm run juno:emulator:start`);
  console.log("2. Open http://localhost:5866 and confirm the local Satellite exists.");
  console.log(`3. cd ${wrapperViteDir} && npm run juno:functions:build`);
  console.log(`4. cd ${wrapperViteDir} && npm run dev`);
  console.log("5. Open the wrapper-vite UI and confirm manual request_id open works.");
  console.log("6. Connect Oisy and confirm principal display in the wallet modal.");
  console.log("7. Confirm Recent Requests shows the new request.");
  console.log("8. Reload and confirm the same principal sees the same request.");
  console.log("9. Submit one wrap and confirm Balance/MAX update after success.");
  console.log("10. When done, run: cd tools/wrapper-vite && npm run juno:emulator:stop");
} catch (error) {
  const message = error instanceof Error ? error.message : "local_wrapper_smoke_failed";
  console.error(message);
  if (message.startsWith("missing_env_file:")) {
    console.error("prepare tools/wrapper-vite/.env.local before running this helper.");
  }
  process.exitCode = 1;
}
