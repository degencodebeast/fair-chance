// @vitest-environment jsdom
// Review finding: the buy sent an amount and a minimum the page did not show. Now it sends exactly the previewed buy, or nothing.
import { describe, expect, it, vi } from "vitest";
import { loadBuyQuote } from "../../src/buy-quote.ts";
import { drawTradeBox } from "../../src/render-trade-box.ts";
import { sendBuy } from "../../src/trade-actions.ts";
import type { TxRequest } from "../../src/tx-types.ts";
import { walletControl } from "../../src/wallet-session.ts";

const COIN = "moth.factory.testnet";
const q = (tokens: string) => ({ tokens_out: tokens, fee_near: "1", creator_fee_near: "1", marked_tokens: "0", class_after: "small", becomes_large: false, allowance_after: "1" });

async function setup(quoteAtClick: string) {
  const sent: TxRequest[][] = [];
  let tokens = "5600000000000000000000000";
  const chain = {
    view: async (m: string, a?: { near_in?: string }): Promise<any> => {
      if (m === "fc_quote_buy") return q(tokens);
      if (m === "storage_balance_of") return { total: "1", available: "0" };
      throw new Error(m);
    },
  };
  const wallet = walletControl("https://x.test/?network=testnet", () => ({
    connect: async () => ({ accountId: "you.testnet", network: "testnet" }),
    restore: async () => null,
    send: async (_a, txs) => (sent.push(txs), []),
    signOut: async () => {}, onSignIn: () => {}, onSignOut: () => {},
  }));
  await wallet.connect();
  const root = drawTradeBox({
    symbol: "MOTH", decimals: 18, largeThresholdBps: 200, storageMin: { kind: "value", value: 1n }, wallet,
    loadQuote: (acc, n) => loadBuyQuote(chain, acc, n),
    blocked: null,
    actions: { buy: (s, b) => sendBuy({ chain, session: s, coin: COIN }, b), sell: async () => ({ kind: "not-sent", text: "" }), sellLimit: async () => ({ balance: 0n, canSellNow: 0n, moreOpens: null }), onTraded: () => {} },
  });
  document.body.replaceChildren(root);
  root.querySelector<HTMLInputElement>("#trade-buy-amount")!.value = "1";
  root.querySelector<HTMLFormElement>("#trade-buy-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
  await vi.waitFor(() => expect(root.querySelector("#trade-buy-status")!.textContent).toBe("Preview for you.testnet."));
  tokens = quoteAtClick;
  return { root, sent };
}

describe("buy preview and sent buy", () => {
  it("sends nothing when the amount box no longer shows the previewed amount", async () => {
    const { root, sent } = await setup("5600000000000000000000000");
    root.querySelector<HTMLInputElement>("#trade-buy-amount")!.value = "0.1";
    root.querySelector<HTMLButtonElement>("#trade-buy-button")!.click();
    await new Promise((r) => setTimeout(r, 20));
    expect(sent).toEqual([]);
    expect(root.querySelector<HTMLButtonElement>("#trade-buy-button")!.disabled).toBe(true);
  });

  it("sends the minimum of the preview the visitor saw, not of a later quote", async () => {
    const { root, sent } = await setup("1000000000000000000000000");
    root.querySelector<HTMLButtonElement>("#trade-buy-button")!.click();
    await vi.waitFor(() => expect(sent.length).toBe(1));
    const call = (sent[0]!.at(-1)!.actions[0] as any).params;
    expect(call.args.min_tokens_out).toBe("5544000000000000000000000");
    expect(call.deposit).toBe("1000000000000000000000000");
  });
});
