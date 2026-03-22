declare module "@dfinity/ic-pub-key/dist/signer/eth.js" {
  import { Principal } from "@icp-sdk/core/principal";

  export type ChainFusionSignerEthAddressResponse = {
    request: unknown;
    response: { eth_address: string };
  };

  export function chainFusionSignerEthAddressFor(user: Principal): ChainFusionSignerEthAddressResponse;
}
