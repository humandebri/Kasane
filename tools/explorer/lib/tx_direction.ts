// どこで: Explorer表示層のTx補助 / 何を: 一覧表示向けのDirection(out/self)を決定 / なぜ: 複数画面で同じ判定を使い誤表示を防ぐため

export function deriveTxDirection(fromAddress: Buffer, toAddress: Buffer | null): "out" | "self" {
  if (toAddress && fromAddress.equals(toAddress)) {
    return "self";
  }
  return "out";
}
