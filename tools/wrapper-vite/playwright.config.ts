// どこで: wrapper-vite Playwright 設定 / 何を: 最小E2Eを Vite dev server 上で実行する / なぜ: 主要画面の回帰を手元で素早く検知するため

import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "./tests/e2e",
  timeout: 30_000,
  use: {
    baseURL: "http://127.0.0.1:4173",
    trace: "on-first-retry",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 4173",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: true,
  },
});
