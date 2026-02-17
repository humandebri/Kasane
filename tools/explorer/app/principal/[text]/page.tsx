// どこで: Principalルート / 何を: 導出したEVMアドレスへリダイレクト / なぜ: Addressページに表示を統合するため

import { notFound, redirect } from "next/navigation";
import { Principal } from "@dfinity/principal";
import { deriveEvmAddressFromPrincipal } from "../../../lib/principal";

export const dynamic = "force-dynamic";

export default async function PrincipalRedirectPage({ params }: { params: Promise<{ text: string }> }) {
  const { text } = await params;
  if (!isValidPrincipal(text)) {
    notFound();
  }
  const addressHex = deriveEvmAddressFromPrincipal(text);
  redirect(`/address/${addressHex}?principal=${encodeURIComponent(text)}`);
}

function isValidPrincipal(value: string): boolean {
  try {
    Principal.fromText(value);
    return true;
  } catch {
    return false;
  }
}
