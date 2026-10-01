// どこで: ブラウザ polyfill / 何を: Vite 起動前に Buffer を global へ注入 / なぜ: ic-pub-key の browser 実行で Buffer が必要なため

import { Buffer } from "buffer/";

if (typeof globalThis.Buffer === "undefined") {
  Reflect.set(globalThis, "Buffer", Buffer);
}

// SDK の静的 import を評価する前に Buffer の初期化を完了させる。
void import("../main");
