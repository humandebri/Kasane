// どこで: Explorer表示層の共通整形 / 何を: 日時やステータスの表示文字列を統一 / なぜ: ページごとの差分を減らして保守性を上げるため

export function formatTimestampUtc(raw: bigint): string {
  // indexer由来timestampは秒想定だが、将来ミリ秒が来ても表示崩れを避ける。
  const millis = raw > 10_000_000_000n ? raw : raw * 1000n;
  if (millis > BigInt(Number.MAX_SAFE_INTEGER)) {
    return raw.toString();
  }
  return new Date(Number(millis)).toISOString().replace("T", " ").replace("Z", " UTC");
}

export function receiptStatusLabel(status: number | null): string {
  if (status === null) {
    return "unknown";
  }
  return status === 1 ? "success" : "failed";
}

export function formatIcpAmountFromWei(value: bigint): string {
  const base = 1_000_000_000_000_000_000n;
  const sign = value < 0n ? "-" : "";
  const abs = value < 0n ? -value : value;
  const whole = abs / base;
  const fractionRaw = (abs % base).toString().padStart(18, "0");
  const fraction = fractionRaw.slice(0, 6).replace(/0+$/, "");
  return `${sign}${whole.toString()}${fraction.length > 0 ? `.${fraction}` : ""} ICP`;
}

export function formatTokenAmount(amount: bigint, decimals: number | null): string {
  if (decimals === null || decimals < 0) {
    return amount.toString();
  }
  const base = 10n ** BigInt(decimals);
  if (base === 0n) {
    return amount.toString();
  }
  const sign = amount < 0n ? "-" : "";
  const abs = amount < 0n ? -amount : amount;
  const whole = abs / base;
  const rawFraction = (abs % base).toString().padStart(decimals, "0");
  const trimmedFraction = rawFraction.replace(/0+$/, "");
  if (trimmedFraction.length === 0) {
    return `${sign}${whole.toString()}`;
  }
  const shownFraction = trimmedFraction.slice(0, 8);
  return `${sign}${whole.toString()}.${shownFraction}`;
}

export function formatEthFromWei(value: bigint, maxFractionDigits = 18): string {
  const base = 1_000_000_000_000_000_000n;
  const sign = value < 0n ? "-" : "";
  const abs = value < 0n ? -value : value;
  const whole = abs / base;
  const fractionRaw = (abs % base).toString().padStart(18, "0");
  const fractionTrimmed = fractionRaw.replace(/0+$/, "").slice(0, maxFractionDigits);
  const fraction = fractionTrimmed.length === 0 ? "" : `.${fractionTrimmed}`;
  return `${sign}${whole.toString()}${fraction} ICP`;
}

export function formatGweiFromWei(value: bigint, maxFractionDigits = 9): string {
  const base = 1_000_000_000n;
  const sign = value < 0n ? "-" : "";
  const abs = value < 0n ? -value : value;
  const whole = abs / base;
  const fractionRaw = (abs % base).toString().padStart(9, "0");
  const fractionTrimmed = fractionRaw.replace(/0+$/, "").slice(0, maxFractionDigits);
  const fraction = fractionTrimmed.length === 0 ? "" : `.${fractionTrimmed}`;
  return `${sign}${whole.toString()}${fraction} Gwei`;
}

export function calcRoundedBps(numerator: bigint, denominator: bigint): bigint | null {
  if (denominator === 0n) {
    return null;
  }
  const absNumerator = numerator < 0n ? -numerator : numerator;
  const roundedAbs = (absNumerator * 10_000n + denominator / 2n) / denominator;
  return numerator < 0n ? -roundedAbs : roundedAbs;
}

export function formatTimestampWithRelativeUtc(raw: bigint | null): { relative: string; absolute: string } | null {
  if (raw === null) {
    return null;
  }
  const millis = raw > 10_000_000_000n ? raw : raw * 1000n;
  if (millis > BigInt(Number.MAX_SAFE_INTEGER)) {
    return { relative: "unknown", absolute: `${raw.toString()} UTC` };
  }
  const date = new Date(Number(millis));
  const nowSec = BigInt(Math.floor(Date.now() / 1000));
  const tsSec = raw > 10_000_000_000n ? raw / 1000n : raw;
  const diffSec = tsSec - nowSec;
  const relative = formatRelativeAge(diffSec);

  const month = date.toLocaleString("en-US", { month: "short", timeZone: "UTC" });
  const day = String(date.getUTCDate()).padStart(2, "0");
  const year = date.getUTCFullYear().toString();
  const hour24 = date.getUTCHours();
  const hour12 = hour24 % 12 === 0 ? 12 : hour24 % 12;
  const hour = String(hour12).padStart(2, "0");
  const minute = String(date.getUTCMinutes()).padStart(2, "0");
  const second = String(date.getUTCSeconds()).padStart(2, "0");
  const ampm = hour24 < 12 ? "AM" : "PM";
  const absolute = `${month}-${day}-${year} ${hour}:${minute}:${second} ${ampm} UTC`;

  return { relative, absolute };
}

function formatRelativeAge(diffSec: bigint): string {
  const isFuture = diffSec > 0n;
  const absSec = diffSec < 0n ? -diffSec : diffSec;
  if (absSec < 60n) {
    return isFuture ? `in ${absSec.toString()}s` : `${absSec.toString()}s ago`;
  }
  if (absSec < 3600n) {
    const value = absSec / 60n;
    return isFuture ? `in ${value.toString()} mins` : `${value.toString()} mins ago`;
  }
  if (absSec < 86_400n) {
    const value = absSec / 3600n;
    return isFuture ? `in ${value.toString()} hrs` : `${value.toString()} hrs ago`;
  }
  const value = absSec / 86_400n;
  return isFuture ? `in ${value.toString()} days` : `${value.toString()} days ago`;
}
