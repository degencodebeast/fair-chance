// Reads the amount a trader types into token base units. Returns null for anything that is not a
// positive amount with at most `decimals` decimal places. Commas are dropped as thousands
// separators ("30,000,000"), so a field that sends a transaction refuses a comma first with
// COMMA_REFUSAL: there "1,5" would read as 15.

/** The sentence for an amount typed with a comma. */
export const COMMA_REFUSAL = "Use a dot for decimals.";

export function hasComma(text: string): boolean {
  return text.includes(",");
}

export function parseTokenAmount(text: string, decimals: number): bigint | null {
  const clean = text.trim().replaceAll(",", "");
  const match = /^(\d+)(?:\.(\d+))?$/.exec(clean);
  if (match === null) return null;
  const whole = match[1] ?? "";
  const fraction = match[2] ?? "";
  if (fraction.length > decimals) return null;
  const units = BigInt(whole) * 10n ** BigInt(decimals) + BigInt(fraction.padEnd(decimals, "0") || "0");
  return units > 0n ? units : null;
}
