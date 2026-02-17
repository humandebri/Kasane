// どこで: 検索ページ / 何を: block番号/tx hash を判別して詳細へ転送 / なぜ: 運用時の導線を短縮するため

import { redirect } from "next/navigation";
import { resolveSearchRoute } from "../../lib/search";

export const dynamic = "force-dynamic";

export default async function SearchPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string }>;
}) {
  const { q } = await searchParams;
  redirect(resolveSearchRoute(q ?? ""));
}
