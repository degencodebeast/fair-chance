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
}

async function optional<T>(read: () => Promise<unknown>, parse: (value: unknown) => T): Promise<Reading<T>> {
  try {
    return { kind: "value", value: parse(await read()) };
  } catch (e) {
    if (e instanceof ViewNotAvailable) return { kind: "unavailable" };
    throw e;
  }
}

/** Any failure other than a missing view is thrown, so the page shows an error and no partial numbers. */
export async function loadAccountPlan(
  chain: ChainReader,
  ask: { accountId: string; asked: bigint | null },
): Promise<AccountPlanReads> {
  const account = await optional(() => chain.view("fc_account", { account_id: ask.accountId }), parseAccount);
  const asked = ask.asked;
  const quote =
    asked === null
      ? null
      : await optional(() => chain.view("fc_quote_sell", { account_id: ask.accountId, amount: asked.toString() }), parseSellQuote);
  return { account, asked, quote };
}
