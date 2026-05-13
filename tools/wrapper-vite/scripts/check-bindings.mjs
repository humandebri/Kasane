// どこで: wrapper-vite declarations check
// 何を: tracked declarations が current DID と一致するか検証する
// なぜ: canister 側の変更が frontend へ反映漏れしても CI/ローカルで即座に検知するため

import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { tmpdir } from "node:os";

const wrapperViteDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bindgenBin = resolve(wrapperViteDir, "node_modules/.bin/icp-bindgen");

const targets = [
  {
    canisterName: "evm_canister",
    didFile: resolve(wrapperViteDir, "../../crates/ic-evm-gateway/evm_canister.did"),
    outDir: resolve(wrapperViteDir, "src/declarations/evm_canister"),
  },
];

function ensureExists(path) {
  if (!existsSync(path)) {
    throw new Error(`bindgen.missing:${path}`);
  }
}

function readUtf8(path) {
  return readFileSync(path, "utf8");
}

function compareTarget(target) {
  ensureExists(target.didFile);
  ensureExists(target.outDir);
  const tempDir = mkdtempSync(join(tmpdir(), `wrapper-vite-${target.canisterName}-`));
  try {
    execFileSync(bindgenBin, [
      "--did-file",
      target.didFile,
      "--out-dir",
      tempDir,
      "--actor-disabled",
      "--force",
    ], {
      cwd: wrapperViteDir,
      stdio: "inherit",
    });

    const generatedDidDts = readUtf8(resolve(tempDir, "declarations", `${target.canisterName}.did.d.ts`));
    const generatedDidJs = readUtf8(resolve(tempDir, "declarations", `${target.canisterName}.did.js`));
    const trackedDidDts = readUtf8(resolve(target.outDir, `${target.canisterName}.did.d.ts`));
    const trackedDidJs = readUtf8(resolve(target.outDir, `${target.canisterName}.did.js`));

    if (generatedDidDts !== trackedDidDts || generatedDidJs !== trackedDidJs) {
      throw new Error(`bindgen.outdated:${target.canisterName}`);
    }
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

for (const target of targets) {
  compareTarget(target);
}
