// どこで: principal変換ユーティリティ / 何を: principalとEVMアドレス変換を提供 / なぜ: request_id導出と表示で同一変換を使うため

import { Principal } from "@dfinity/principal";
import { chainFusionSignerEthAddressFor } from "@dfinity/ic-pub-key/dist/signer/eth.js";

export function principalTextToBytes(text: string): Uint8Array {
  return Principal.fromText(text).toUint8Array();
}

export function principalBytesToText(bytes: Uint8Array): string {
  return Principal.fromUint8Array(bytes).toText();
}

export function callerEvmAddressFromPrincipalText(principalText: string): Uint8Array {
  const principal = Principal.fromText(principalText);
  const out = chainFusionSignerEthAddressFor(principal);
  const hex = out.response.eth_address.toLowerCase();
  const normalized = hex.startsWith("0x") ? hex.slice(2) : hex;
  return Uint8Array.from(Buffer.from(normalized, "hex"));
}
