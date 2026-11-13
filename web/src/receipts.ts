// Receipt rows for the coin page, as a pure function of chain events.
import type { CoinEvent } from "./chain.ts";

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
const NEAR_DECIMALS = 24;

export function receiptRows(events: CoinEvent[]): ReceiptRow[] {
  return events
    .filter((e) => e.standard === "fair_chance" && e.name === "fc_sell")
    .map((e) => {
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
      const text =
        `${row.account} sold ${amount(row.tokens_sold, TOKEN_DECIMALS, 2)} of ` +
        `${amount(row.tokens_asked, TOKEN_DECIMALS, 2)} tokens for ` +
        `${amount(row.near_out, NEAR_DECIMALS, 4)} NEAR. Fill: ${row.fill}.`;
      return { ...row, text };
    });
}

/** Base units to whole units, cut (not rounded) to `places` decimals, with thousands commas. */
export function amount(base: string, decimals: number, places: number): string {
  if (!/^\d+$/.test(base)) return base;
  const padded = base.padStart(decimals + 1, "0");
  const whole = padded.slice(0, padded.length - decimals).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return `${whole}.${padded.slice(padded.length - decimals).slice(0, places)}`;
}
