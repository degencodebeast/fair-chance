// Typed reading of fc_quote_buy. The page shows these fields and does no buy math of its own.
import { fields, reader, UnexpectedView } from "./coin-views.ts";
import type { AccountClass } from "./account-views.ts";
import { ViewNotAvailable, type ChainReader } from "./chain.ts";
import type { Reading } from "./coin-page.ts";

export interface BuyQuote {
  tokensOut: bigint;
  feeNear: bigint;
  creatorFeeNear: bigint;
  markedTokens: bigint;
  classAfter: AccountClass;
  /** True when the buy takes an account that is not large now to the large threshold or more. */
  becomesLarge: boolean;
  allowanceAfter: bigint;
}

export function parseBuyQuote(value: unknown): BuyQuote {
  const r = reader("fc_quote_buy", fields(value, "fc_quote_buy"));
  const classAfter = r.text("class_after");
  if (classAfter !== "small" && classAfter !== "band" && classAfter !== "large") {
    throw new UnexpectedView("fc_quote_buy.class_after is not small, band or large");
  }
  return {
    tokensOut: r.amount("tokens_out"),
    feeNear: r.amount("fee_near"),
    creatorFeeNear: r.amount("creator_fee_near"),
    markedTokens: r.amount("marked_tokens"),
    classAfter,
    becomesLarge: r.flag("becomes_large"),
    allowanceAfter: r.amount("allowance_after"),
  };
}

/** Reads the quote for one buy. A coin without the view gives "unavailable"; any other failure is thrown. */
export async function loadBuyQuote(chain: ChainReader, accountId: string, nearIn: bigint): Promise<Reading<BuyQuote>> {
  try {
    return { kind: "value", value: parseBuyQuote(await chain.view("fc_quote_buy", { account_id: accountId, near_in: nearIn.toString() })) };
  } catch (e) {
    if (e instanceof ViewNotAvailable) return { kind: "unavailable" };
    throw e;
  }
}
