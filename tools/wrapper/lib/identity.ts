// どこで: submit署名identity / 何を: 環境変数の秘密鍵からEd25519 identityを生成 / なぜ: update call を匿名ではなく署名付きで実行するため

import type { Identity } from "@dfinity/agent";
import { Ed25519KeyIdentity } from "@dfinity/identity";

function decodeSecretHex(secretHex: string): Uint8Array {
  const normalized = secretHex.startsWith("0x") ? secretHex.slice(2) : secretHex;
  if (!/^[0-9a-fA-F]{64}$/.test(normalized)) {
    throw new Error("config.invalid:ICP_IDENTITY_SECRET_KEY_HEX");
  }
  return Uint8Array.from(Buffer.from(normalized, "hex"));
}

export function submitIdentityFromSecretHex(secretHex: string): Identity {
  const secret = decodeSecretHex(secretHex);
  return Ed25519KeyIdentity.fromSecretKey(secret);
}

export function submitPrincipalTextFromSecretHex(secretHex: string): string {
  return submitIdentityFromSecretHex(secretHex).getPrincipal().toText();
}
