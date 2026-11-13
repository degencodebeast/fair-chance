// Reads the amount a trader types into token base units. Returns null for anything that is not a
// positive amount with at most `decimals` decimal places.

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
