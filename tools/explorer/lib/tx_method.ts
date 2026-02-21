// どこで: Explorer表示層のTx補助 / 何を: Tx selectorからMethod表示名を推定 / なぜ: 複数画面で同一ロジックを再利用するため

export function inferMethodLabel(toHex: string | null, txSelector: Buffer | null): string {
  if (toHex === null) {
    return "create";
  }
  if (!txSelector || txSelector.length !== 4) {
    return "call";
  }
  const selector = txSelector.toString("hex");
  const known = selectorToMethodName(selector);
  return known ?? `0x${selector}`;
}

export function shortenMethodLabel(label: string, keep: number = 10): string {
  if (label.length <= keep) {
    return label;
  }
  return `${label.slice(0, keep)}...`;
}

function selectorToMethodName(selectorHex: string): string | null {
  const normalized = selectorHex.toLowerCase();
  return METHOD_BY_SELECTOR[normalized] ?? null;
}

const METHOD_BY_SELECTOR: Record<string, string> = {
  // ERC-20
  a9059cbb: "transfer",
  "095ea7b3": "approve",
  "23b872dd": "transferFrom",
  "70a08231": "balanceOf",
  dd62ed3e: "allowance",
  "313ce567": "decimals",
  "95d89b41": "symbol",
  "06fdde03": "name",
  "18160ddd": "totalSupply",
  "40c10f19": "mint",
  "42966c68": "burn",
  d505accf: "permit",

  // Ownership / Access
  f2fde38b: "transferOwnership",
  "715018a6": "renounceOwnership",

  // UniswapV2 Router common
  "38ed1739": "swapExactTokensForTokens",
  "8803dbee": "swapTokensForExactTokens",
  "7ff36ab5": "swapExactETHForTokens",
  "18cbafe5": "swapExactTokensForETH",
  e8e33700: "addLiquidity",
  f305d719: "addLiquidityETH",
  "5c11d795": "removeLiquidityETHSupportingFeeOnTransferTokens",

  // Wrapped native token (WICP on Kasane/ICP EVM)
  d0e30db0: "deposit",
  "2e1a7d4d": "withdraw",

  // Multicall
  ac9650d8: "multicall",
};
