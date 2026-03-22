// どこで: Vite エントリ / 何を: ルーターとグローバル CSS を起動 / なぜ: Next.js 依存なしで dashboard を描画するため

import { StrictMode } from "react";
import { Buffer } from "buffer/";
import { createRoot } from "react-dom/client";
import { AppRouter } from "@/src/app/router";
import "@/src/styles/globals.css";

const container = document.getElementById("root");

if (typeof globalThis.Buffer === "undefined") {
  Reflect.set(globalThis, "Buffer", Buffer);
}

if (!container) {
  throw new Error("app.root_missing");
}

createRoot(container).render(
  <StrictMode>
    <AppRouter />
  </StrictMode>,
);
