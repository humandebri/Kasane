// どこで: wrapper-vite bindgen スクリプト
// 何を: gateway canister の generated declarations を再生成する
// なぜ: 手書き IDL を廃止し、frontend の actor 定義 drift を防ぐため

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
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

function generateTarget(target) {
  ensureExists(target.didFile);
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
    mkdirSync(target.outDir, { recursive: true });
    copyFileSync(
      resolve(tempDir, "declarations", `${target.canisterName}.did.d.ts`),
      resolve(target.outDir, `${target.canisterName}.did.d.ts`),
    );
    copyFileSync(
      resolve(tempDir, "declarations", `${target.canisterName}.did.js`),
      resolve(target.outDir, `${target.canisterName}.did.js`),
    );
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

for (const target of targets) {
  generateTarget(target);
}
