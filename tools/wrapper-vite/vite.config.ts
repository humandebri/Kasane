// どこで: Vite 設定 / 何を: React・Juno・alias をまとめて定義 / なぜ: wrapper dashboard を Juno 配備前提の SPA として動かすため

import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import juno from "@junobuild/vite-plugin";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), juno()],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) {
            return undefined;
          }
          if (
            id.includes("@junobuild/core")
            || id.includes("@icp-sdk/core")
            || id.includes("@dfinity/ic-pub-key")
          ) {
            return "auth-sdk";
          }
          if (
            id.includes("react-router-dom")
            || id.includes("lucide-react")
            || id.includes("@radix-ui/")
            || id.includes("/cmdk/")
          ) {
            return "ui-vendor";
          }
          return undefined;
        },
      },
    },
  },
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./", import.meta.url)),
      buffer: "buffer/",
    },
  },
});
