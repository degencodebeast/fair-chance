import { describe, expect, it } from "vitest";
import { parseBuyQuote } from "../src/buy-quote.ts";
import { UnexpectedView } from "../src/coin-views.ts";

const view = {
  tokens_out: "5600000000000000000000000",
  fee_near: "20000000000000000000000",
  creator_fee_near: "6000000000000000000000",
  marked_tokens: "0",
  class_after: "small",
  becomes_large: false,
  allowance_after: "7100000000000000000000000",
};

describe("fc_quote_buy", () => {
  it("reads every field of the view", () => {
    expect(parseBuyQuote(view)).toEqual({
      tokensOut: 5_600_000_000_000_000_000_000_000n,
      feeNear: 20_000_000_000_000_000_000_000n,
      creatorFeeNear: 6_000_000_000_000_000_000_000n,
      markedTokens: 0n,
      classAfter: "small",
      becomesLarge: false,
      allowanceAfter: 7_100_000_000_000_000_000_000_000n,
    });
  });

  it("takes becomes_large from the view", () => {
    expect(parseBuyQuote({ ...view, becomes_large: true, class_after: "large" }).becomesLarge).toBe(true);
  });

  it("refuses a view with a wrong field", () => {
    expect(() => parseBuyQuote({ ...view, becomes_large: "yes" })).toThrow(UnexpectedView);
    expect(() => parseBuyQuote({ ...view, class_after: "huge" })).toThrow(UnexpectedView);
    expect(() => parseBuyQuote(null)).toThrow(UnexpectedView);
  });
});
