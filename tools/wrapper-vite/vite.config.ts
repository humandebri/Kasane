// どこで: Vite 設定 / 何を: React・alias・chunk 分割を定義 / なぜ: wrapper dashboard を静的SPAとして配備するため

import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes("node_modules")) {
            return undefined;
          }
          if (
            id.includes("@icp-sdk/core")
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
