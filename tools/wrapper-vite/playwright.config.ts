// どこで: wrapper-vite Playwright 設定 / 何を: 最小E2Eを production build 上で実行する / なぜ: 配備される成果物とツールチェーンの回帰を検知するため

import { defineConfig } from "@playwright/test";
import { readFileSync } from "node:fs";
import { parseEnv } from "node:util";

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 30_000,
  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "on-first-retry",
  },
  webServer: {
    command: "npm run build && npm run preview -- --host 127.0.0.1 --port 4173",
    // 開発者の .env.local に依存せず、追跡済みの token list fixture を使う。
    env: Object.fromEntries(
      Object.entries(
        parseEnv(readFileSync(new URL("./.env.example", import.meta.url), "utf8")),
      ).filter((entry): entry is [string, string] => entry[1] !== undefined),
    ),
    url: "http://127.0.0.1:4173",
    reuseExistingServer: false,
  },
});
