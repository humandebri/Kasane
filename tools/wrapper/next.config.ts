// どこで: Next.js設定 / 何を: wrapperダッシュボードの最小設定 / なぜ: 既定挙動で安定運用するため

import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  reactStrictMode: true,
};

export default nextConfig;
