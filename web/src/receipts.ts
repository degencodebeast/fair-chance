// Receipt rows for the coin page, as a pure function of chain events.
import type { CoinEvent } from "./chain.ts";
import { compactTokens, nearAmount } from "./format.ts";

export interface ReceiptRow {
  receipt_id: string;
  block: number;
  account: string;
  tokens_asked: string;
  tokens_sold: string;
  near_out: string;
  fill: string;
  limit_reason: string | null;
  text: string;
}

const TOKEN_DECIMALS = 18;

export function receiptRows(events: CoinEvent[]): ReceiptRow[] {
  return events.filter((e) => e.standard === "fair_chance" && e.name === "fc_sell").map(sellRow);
}

/** One `fc_sell` event as a receipt row. */
export function sellRow(e: CoinEvent): ReceiptRow {
  const d = e.data as Record<string, string | null>;
  const row = {
    receipt_id: e.receiptId,
    block: e.block,
    account: String(d.account),
    tokens_asked: String(d.tokens_asked),
    tokens_sold: String(d.tokens_sold),
    near_out: String(d.near_out),
    fill: String(d.fill),
    limit_reason: d.limit_reason ?? null,
  };
  const [sold, asked] = tokensPair(row.tokens_sold, row.tokens_asked);
  const text = `${row.account} sold ${sold} of ${asked} tokens for ${nearShort(row.near_out)} NEAR.${FILL_WORDS[row.fill] ?? ""}`;
  return { ...row, text };
}

const FILL_WORDS: Record<string, string> = {
  full: " It filled in full.",
  partial: " It filled in part.",
  none: " Nothing filled.",
};

/** Token base units as a compact amount: 7.1M, 100K. A nonzero amount below 0.1 reads "less than 0.1". */
export function tokensShort(base: string): string {
  if (!/^\d+$/.test(base)) return base;
  const text = compactTokens(base, TOKEN_DECIMALS);
  return text === "0.0" ? "less than 0.1" : text;
}

/**
 * Two token amounts as compact text. When the amounts differ but their compact forms are equal
 * ("4.9M of 4.9M"), more digits are shown until the two texts differ, so a part fill never reads as full.
 */
export function tokensPair(sold: string, asked: string): [string, string] {
  const compact: [string, string] = [tokensShort(sold), tokensShort(asked)];
  if (compact[0] !== compact[1] || sold === asked || !/^\d+$/.test(sold) || !/^\d+$/.test(asked)) return compact;
  for (const places of [2, 6, TOKEN_DECIMALS]) {
    const longer: [string, string] = [amount(sold, TOKEN_DECIMALS, places), amount(asked, TOKEN_DECIMALS, places)];
    if (longer[0] !== longer[1]) return longer;
  }
  return [sold, asked];
}

/** yoctoNEAR as NEAR with 2 decimals. A nonzero amount below 0.01 reads "less than 0.01". */
export function nearShort(yocto: string): string {
  if (!/^\d+$/.test(yocto)) return yocto;
  const text = nearAmount(yocto, 2);
  return text === "0.00" && BigInt(yocto) > 0n ? "less than 0.01" : text;
}

/** Base units to whole units, cut (not rounded) to `places` decimals, with thousands commas. */
export function amount(base: string, decimals: number, places: number): string {
  if (!/^\d+$/.test(base)) return base;
  const padded = base.padStart(decimals + 1, "0");
  const whole = padded.slice(0, padded.length - decimals).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${whole}.${padded.slice(padded.length - decimals).slice(0, places)}`;
}
