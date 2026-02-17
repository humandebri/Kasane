// どこで: Principal導出ユーティリティ / 何を: principalからChain Fusion Signer互換のEVMアドレスを導出 / なぜ: principalページでAddress相当情報を表示するため

import { Principal } from "@dfinity/principal";
import { chainFusionSignerEthAddressFor } from "@dfinity/ic-pub-key/dist/signer/eth.js";

export function deriveEvmAddressFromPrincipal(principalText: string): string {
  const principal = Principal.fromText(principalText);
  const out = chainFusionSignerEthAddressFor(principal);
  return out.response.eth_address.toLowerCase();
}
