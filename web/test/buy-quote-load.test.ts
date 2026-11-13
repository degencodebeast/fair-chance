import { describe, expect, it, vi } from "vitest";
import { ViewNotAvailable, type ChainReader } from "../src/chain.ts";
import { loadBuyQuote } from "../src/buy-quote.ts";

const view = {
  tokens_out: "1", fee_near: "0", creator_fee_near: "0", marked_tokens: "0", class_after: "small", becomes_large: true, allowance_after: "0",
};
const reader = (view_: ChainReader["view"]): ChainReader => ({ view: view_, account: vi.fn(), accessKeyCount: vi.fn(), events: vi.fn() } as unknown as ChainReader);

describe("loadBuyQuote", () => {
  it("calls fc_quote_buy with the account and the NEAR in yoctoNEAR as text", async () => {
    const call = vi.fn(async () => view);
    const got = await loadBuyQuote(reader(call as ChainReader["view"]), "you.testnet", 1_500n);
    expect(call).toHaveBeenCalledWith("fc_quote_buy", { account_id: "you.testnet", near_in: "1500" });
    expect(got).toMatchObject({ kind: "value", value: { becomesLarge: true } });
  });

  it("says unavailable for a coin without the view, and passes other failures on", async () => {
    expect(await loadBuyQuote(reader(async () => { throw new ViewNotAvailable("fc_quote_buy"); }), "a.testnet", 1n)).toEqual({ kind: "unavailable" });
    await expect(loadBuyQuote(reader(async () => { throw new Error("rpc down"); }), "a.testnet", 1n)).rejects.toThrow("rpc down");
  });
});
