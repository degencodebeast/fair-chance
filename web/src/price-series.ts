// The price of each trade, read from the coin's swap events. This is the price a trade got
// (fees included on a buy, net of fees on a sell), not a pool reading, so the chart says so.
import type { CoinEvent } from "./chain.ts";

const NEAR_UNIT = 10n ** 24n;
const MILLION_TOKENS = 10n ** 24n; // 1M tokens in base units (18 decimals)

export interface PricePoint {
  block: number;
  timeNs: string | null;
  /** NEAR for 1M tokens. */
  nearPerMillion: number;
}

function big(value: unknown): bigint {
  try {
    return typeof value === "string" ? BigInt(value) : 0n;
  } catch {
    return 0n;
  }
}

function perMillion(near: bigint, tokens: bigint): number {
  const yocto = (near * MILLION_TOKENS) / tokens;
  // Parse a decimal text so 0.2 stays 0.2 and not 0.19999999999999998.
  return Number(`${yocto / NEAR_UNIT}.${(yocto % NEAR_UNIT).toString().padStart(24, "0")}`);
}

export function priceSeries(events: CoinEvent[]): PricePoint[] {
  const points: PricePoint[] = [];
  for (const e of events) {
    let near = 0n;
    let tokens = 0n;
    if (e.name === "fc_buy") [near, tokens] = [big(e.data.near_in), big(e.data.tokens_out)];
    else if (e.name === "fc_sell") [near, tokens] = [big(e.data.near_out), big(e.data.tokens_sold)];
    else continue;
    if (tokens === 0n || near === 0n) continue;
    points.push({ block: e.block, timeNs: e.timeNs, nearPerMillion: perMillion(near, tokens) });
  }
  return points.sort((a, b) => a.block - b.block);
}

/** "+25.0%" or "-20.0%", from the first trade to the last; null with fewer than two trades. */
export function changeText(points: PricePoint[]): string | null {
  if (points.length < 2) return null;
  const first = points[0]!.nearPerMillion;
  const last = points[points.length - 1]!.nearPerMillion;
  // Cut, not rounded, to one decimal.
  const tenths = Math.trunc(Math.round(((last - first) / first) * 100000) / 100);
  const sign = tenths < 0 ? "-" : "+";
  const abs = Math.abs(tenths);
  return `${sign}${Math.trunc(abs / 10)}.${abs % 10}%`;
}

/** SVG polyline points: x spread over the width, y inverted so a higher price is higher up. */
export function linePoints(points: PricePoint[], width: number, height: number, margin: number): string {
  if (points.length === 0) return "";
  const prices = points.map((p) => p.nearPerMillion);
  const min = Math.min(...prices);
  const max = Math.max(...prices);
  return points
    .map((p, i) => {
      const x = points.length === 1 ? 0 : (i / (points.length - 1)) * width;
      const y = max === min ? height / 2 : height - margin - ((p.nearPerMillion - min) / (max - min)) * (height - 2 * margin);
      return `${+x.toFixed(1)},${+y.toFixed(1)}`;
    })
    .join(" ");
}
