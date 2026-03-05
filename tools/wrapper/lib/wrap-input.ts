// どこで: wrap入力処理共通 / 何を: nonce/deadline生成と数値検証を提供 / なぜ: UIと送信処理の検証ルールを統一するため

export function randomU64NonceText(): string {
  const words = new Uint32Array(2);
  crypto.getRandomValues(words);
  const hi = words[0] ?? 0;
  const lo = words[1] ?? 0;
  return ((BigInt(hi) << 32n) | BigInt(lo)).toString();
}

export function defaultDeadlineText(): string {
  return (Math.floor(Date.now() / 1000) + 3600).toString();
}

export function parsePositiveBigInt(text: string, code: string): bigint {
  const value = BigInt(text.trim());
  if (value <= 0n) {
    throw new Error(code);
  }
  return value;
}

export function parseU64(text: string, code: string): bigint {
  const value = BigInt(text.trim());
  if (value < 0n || value > 0xffff_ffff_ffff_ffffn) {
    throw new Error(code);
  }
  return value;
}
