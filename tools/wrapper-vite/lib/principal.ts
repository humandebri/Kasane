// どこで: principal変換ユーティリティ / 何を: principalとEVMアドレス変換を提供 / なぜ: request_id導出とcanister update送信時に同一変換を使うため

import { Principal } from "@icp-sdk/core/principal";
import { chainFusionSignerEthAddressFor } from "@dfinity/ic-pub-key/dist/signer/eth.js";
import { hexToBytes } from "./utils";

export function principalTextToBytes(text: string): Uint8Array {
  return Principal.fromText(text).toUint8Array();
}

export function principalBytesToText(bytes: Uint8Array): string {
  return Principal.fromUint8Array(bytes).toText();
}

export function callerEvmAddressFromPrincipalText(principalText: string): Uint8Array {
  const principal = Principal.fromText(principalText);
  const out = chainFusionSignerEthAddressFor(principal);
  return hexToBytes(out.response.eth_address.toLowerCase());
}
