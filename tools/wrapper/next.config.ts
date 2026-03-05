// どこで: Next.js設定 / 何を: wrapperダッシュボードの最小設定と公開環境変数を定義 / なぜ: クライアントからcanister直呼びするため

import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  env: {
    EVM_GATEWAY_CANISTER_ID: process.env.EVM_GATEWAY_CANISTER_ID,
    WRAP_CANISTER_ID: process.env.WRAP_CANISTER_ID,
  },
};

export default nextConfig;
