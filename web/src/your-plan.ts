// Reads one account's plan and, when an amount is given, the sell preview. Every number comes from
// the coin's fc_account and fc_quote_sell views; the page does no rule math of its own.
import { ViewNotAvailable, type ChainReader } from "./chain.ts";
import { parseAccount, parseSellQuote, type Account, type SellQuote } from "./account-views.ts";
import type { Reading } from "./coin-page.ts";

export interface AccountPlanReads {
  account: Reading<Account>;
  asked: bigint | null;
  /** Null when no amount was asked. */
  quote: Reading<SellQuote> | null;
  /**
   * For an account with early supply and other tokens: the quote of N, the early supply it can
   * sell now (the smaller of `marked_balance` and `marked.left`). Null when not read.
   */
  earlySupplyQuote: { amount: bigint; quote: Reading<SellQuote> } | null;
}

async function optional<T>(read: () => Promise<unknown>, parse: (value: unknown) => T): Promise<Reading<T>> {
  try {
    return { kind: "value", value: parse(await read()) };
  } catch (e) {
    if (e instanceof ViewNotAvailable) return { kind: "unavailable" };
    throw e;
  }
}

/** N: the early supply a sell takes before the other tokens; null when the coin does not say. */
export function earlySupplySellFirst(account: Account): bigint | null {
  if (account.markedBalance === 0n || account.unmarkedBalance === 0n || account.marked.kind !== "value") return null;
  const left = account.marked.value.left;
  return account.markedBalance < left ? account.markedBalance : left;
}

/** Any failure other than a missing view is thrown, so the page shows an error and no partial numbers. */
export async function loadAccountPlan(
  chain: ChainReader,
  ask: { accountId: string; asked: bigint | null },
): Promise<AccountPlanReads> {
  const account = await optional(() => chain.view("fc_account", { account_id: ask.accountId }), parseAccount);
  const asked = ask.asked;
  const quoteOf = (amount: bigint) =>
    optional(() => chain.view("fc_quote_sell", { account_id: ask.accountId, amount: amount.toString() }), parseSellQuote);
  const quote = asked === null ? null : await quoteOf(asked);
  const n = asked === null || account.kind !== "value" ? null : earlySupplySellFirst(account.value);
  const earlySupplyQuote =
    n === null || n === 0n || quote === null ? null : { amount: n, quote: n === asked ? quote : await quoteOf(n) };
  return { account, asked, quote, earlySupplyQuote };
}
