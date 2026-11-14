// fc_quote_sell is null for any refused sell (rule-engine/src/views.rs quote_sell: apply(...).ok()?).
// In blocks 0 to N an account with marked tokens is refused with FC_EARLY_WINDOW
// (rule-engine/src/actions.rs sell), so its quote is null even for an amount within its balance.
import { describe, expect, it } from "vitest";
import { loadSellLimit, sendSell } from "../../src/trade-actions.ts";
import type { WalletSession } from "../../src/tx-types.ts";

const SELLER = "early.fairchance.testnet";
const COIN = "bag.factory.fairchance.testnet";
const view = (views: Record<string, unknown>) => ({
  async view<T>(method: string): Promise<T> {
    return views[method] as T;
  },
});
const session: WalletSession = { accountId: SELLER, send: async () => [], signOut: async () => {} };

describe("null quote in the early window", () => {
  it("sendSell of 1 base unit (balance is far above) says the visitor holds less", async () => {
    const out = await sendSell({ chain: view({ fc_quote_sell: null }), session, coin: COIN }, 1n);
    // Intent: wording must not claim what the contract does not do. 1 unit is not above the balance.
    expect(out).toEqual({ kind: "not-sent", text: expect.not.stringContaining("You hold less than this amount") });
  });

  it("loadSellLimit for an early-supply holder says the balance changed", async () => {
    const account = { class: "small", balance: "2709426042412763046439220", marked_balance: "2709426042412763046439220", unmarked_balance: "0", is_large: false, can_send: true };
    const chain = view({ fc_account: account, fc_quote_sell: null });
    // Intent: "you can sell up to X now" comes from the chain views: here 0 now, early window.
    await expect(loadSellLimit(chain, SELLER, { symbol: "BAG", decimals: 24 })).resolves.toMatchObject({ canSellNow: 0n });
  });
});
