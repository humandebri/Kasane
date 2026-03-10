// どこで: Next.js設定 / 何を: wrapperダッシュボードの最小設定と公開環境変数を定義 / なぜ: クライアントからcanister直呼びするため

import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
  env: {
    NEXT_PUBLIC_IC_HOST: process.env.NEXT_PUBLIC_IC_HOST,
    KASANE_EVM_CANISTER_ID: process.env.KASANE_EVM_CANISTER_ID,
    WRAP_CANISTER_ID: process.env.WRAP_CANISTER_ID,
    EVM_WRAP_FACTORY: process.env.EVM_WRAP_FACTORY,
  },
};

export default nextConfig;
