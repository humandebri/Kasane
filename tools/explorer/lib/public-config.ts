export function publicRpcConfig() {
  return {
    canisterId: process.env.EVM_CANISTER_ID ?? null,
    icHost: process.env.EXPLORER_IC_HOST ?? process.env.INDEXER_IC_HOST ?? "https://icp-api.io",
  };
}
