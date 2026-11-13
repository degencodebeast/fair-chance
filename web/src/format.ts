// Number and time text for the coin page. Amounts are decimal strings in base units.

const SCALE: readonly [bigint, string][] = [
  [1_000_000_000n, "B"],
  [1_000_000n, "M"],
  [1_000n, "K"],
];

/** Token base units to a short text: 10.1M, 100K, 1.5. Cut, never rounded up. */
export function compactTokens(base: string, decimals: number): string {
  const units = BigInt(base);
  const whole = units / 10n ** BigInt(decimals);
  for (const [size, suffix] of SCALE) {
    if (whole >= size) {
      const tenths = (units * 10n) / (10n ** BigInt(decimals) * size);
      const exact = suffix === "K" && tenths % 10n === 0n;
      return exact ? `${whole / size}${suffix}` : `${tenths / 10n}.${tenths % 10n}${suffix}`;
    }
  }
  if (units === 0n) return "0";
  const tenths = (units * 10n) / 10n ** BigInt(decimals);
  return `${tenths / 10n}.${tenths % 10n}`;
}

function ceilDiv(a: bigint, b: bigint): bigint {
  return (a + b - 1n) / b;
}

/**
 * Token base units to a short text like compactTokens, but rounded up at the shown precision
 * (never to nearest or down), so the text is at or above the amount: 6.04M shows as 6.1M.
 */
export function compactTokensRoundedUp(base: string, decimals: number): string {
  const units = BigInt(base);
  if (units === 0n) return "0";
  const one = 10n ** BigInt(decimals);
  // Smallest to largest: the first scale whose rounded-up text stays below 1000 of it.
  const scales: readonly [bigint, string][] = [[1n, ""], ...[...SCALE].reverse()];
  for (const [i, [size, suffix]] of scales.entries()) {
    const tenths = ceilDiv(units * 10n, one * size);
    if (tenths < 10_000n || i === scales.length - 1) {
      if (suffix === "K" && tenths % 10n === 0n) return `${tenths / 10n}${suffix}`;
      return `${tenths / 10n}.${tenths % 10n}${suffix}`;
    }
  }
  throw new Error("unreachable");
}

/** yoctoNEAR to NEAR, cut to `places` decimals. */
export function nearAmount(yocto: string, places = 2): string {
  const units = BigInt(yocto);
  const scale = 10n ** 24n;
  const cut = (units * 10n ** BigInt(places)) / scale;
  const digits = cut.toString().padStart(places + 1, "0");
  return `${digits.slice(0, digits.length - places)}.${digits.slice(digits.length - places)}`;
}

export function percentOfBps(bps: number): string {
  return `${(bps / 100).toFixed(1)}%`;
}

const SECOND = 1_000_000_000n;
const MINUTE = 60n * SECOND;
const HOUR = 60n * MINUTE;
const DAY = 24n * HOUR;

/** A length of time: 40 s, 12 min, 1 h 22 min, 2 days 2 h. */
export function age(durationNs: bigint): string {
  const d = durationNs < 0n ? 0n : durationNs;
  if (d < MINUTE) return `${d / SECOND} s`;
  if (d < HOUR) return `${d / MINUTE} min`;
  if (d < DAY) return `${d / HOUR} h ${(d % HOUR) / MINUTE} min`;
  return `${d / DAY} days ${(d % DAY) / HOUR} h`;
}

export interface TimeContext {
  nowNs: bigint;
  /** IANA time zone. Unset: the viewer's own time zone. */
  timeZone?: string;
}

function parts(ns: bigint, timeZone: string | undefined) {
  const f = new Intl.DateTimeFormat("en-GB", {
    timeZone,
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  });
  const get = Object.fromEntries(f.formatToParts(new Date(Number(ns / 1_000_000n))).map((p) => [p.type, p.value]));
  return { clock: `${get.hour}:${get.minute}`, day: `${get.year}-${get.month}-${get.day}` };
}

/** Local 24 h clock time: 15:02. */
export function clockTime(ns: bigint, ctx: Pick<TimeContext, "timeZone">): string {
  return parts(ns, ctx.timeZone).clock;
}

/** Local 24 h time with a day word when it is not today, and a hint: "15:02, in 38 min". */
export function timeWithHint(ns: bigint, ctx: TimeContext): string {
  const at = parts(ns, ctx.timeZone);
  const today = parts(ctx.nowNs, ctx.timeZone).day;
  const dayShift = Math.round((Date.parse(at.day) - Date.parse(today)) / 86_400_000);
  const dayWord = dayShift === 0 ? "" : dayShift === 1 ? " tomorrow" : dayShift === -1 ? " yesterday" : ` ${at.day}`;
  const delta = ns - ctx.nowNs;
  const abs = delta < 0n ? -delta : delta;
  const hint = abs < 5n * SECOND ? "now" : delta > 0n ? `in ${age(abs)}` : `${age(abs)} ago`;
  return `${at.clock}${dayWord}, ${hint}`;
}

/** yoctoNEAR to NEAR in full, without trailing zeros: 0.00125, 1, 1.5. */
export function exactNear(yocto: string): string {
  const units = BigInt(yocto);
  const scale = 10n ** 24n;
  const whole = units / scale;
  const fraction = (units % scale).toString().padStart(24, "0").replace(/0+$/, "");
  return fraction === "" ? whole.toString() : `${whole}.${fraction}`;
}
