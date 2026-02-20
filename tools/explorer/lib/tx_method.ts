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

function selectorToMethodName(selectorHex: string): string | null {
  if (selectorHex === "a9059cbb") return "transfer";
  if (selectorHex === "095ea7b3") return "approve";
  if (selectorHex === "23b872dd") return "transferFrom";
  if (selectorHex === "70a08231") return "balanceOf";
  if (selectorHex === "dd62ed3e") return "allowance";
  if (selectorHex === "313ce567") return "decimals";
  if (selectorHex === "95d89b41") return "symbol";
  if (selectorHex === "06fdde03") return "name";
  return null;
}
