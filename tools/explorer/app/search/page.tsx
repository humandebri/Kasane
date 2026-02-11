// どこで: 検索ページ / 何を: block番号/tx hash を判別して詳細へ転送 / なぜ: 運用時の導線を短縮するため

import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function SearchPage({
  searchParams,
}: {
  searchParams: Promise<{ q?: string }>;
}) {
  const { q } = await searchParams;
  const value = (q ?? "").trim();
  if (/^[0-9]+$/.test(value)) {
    redirect(`/blocks/${value}`);
  }
  if (/^(0x)?[0-9a-fA-F]{64}$/.test(value)) {
    const normalized = value.startsWith("0x") ? value.toLowerCase() : `0x${value.toLowerCase()}`;
    redirect(`/tx/${normalized}`);
  }
  redirect("/");
}
