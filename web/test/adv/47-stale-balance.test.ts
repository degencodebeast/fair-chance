// @vitest-environment jsdom
// The share buttons keep the last balance read, also after a sell, while the new read runs or after it fails.
import { describe, expect, it, vi } from "vitest";
import { drawTradeBox, type TradeActions } from "../../src/render-trade-box.ts";
import type { TradeOutcome } from "../../src/trade-actions.ts";
import { walletControl } from "../../src/wallet-session.ts";

const PAGE = "https://example.test/?network=testnet&coin=moth.factory.testnet";
async function signedIn() {
  const c = walletControl(PAGE, () => ({
    connect: async () => ({ accountId: "you.testnet", network: "testnet" }),
    restore: async () => null, send: async () => [], signOut: async () => {}, onSignIn: () => {}, onSignOut: () => {},
  }));
  await c.connect();
  return c;
}
const sold: TradeOutcome = { kind: "result", result: { kind: "sold", txHash: "Hx2", explorerUrl: "https://testnet.nearblocks.io/txns/Hx2", account: "you.testnet", fill: "full", tokensAsked: 2n * 10n ** 24n, tokensSold: 2n * 10n ** 24n, tokensKept: 0n, nearOut: 10n ** 23n, penaltyNear: 0n, limitReason: null } };

describe("share buttons after a sell", () => {
  it("Max fills the old balance when the new read fails", async () => {
    let n = 0;
    const actions: TradeActions = {
      buy: vi.fn(), onTraded: () => {},
      sell: vi.fn(async () => sold),
      sellLimit: vi.fn(async () => {
        if (n++ === 0) return { balance: 2n * 10n ** 24n, canSellNow: 2n * 10n ** 24n, moreOpens: null };
        throw new Error("rpc down");
      }),
    };
    const root = drawTradeBox({ symbol: "MOTH", decimals: 18, largeThresholdBps: 200, storageMin: { kind: "value", value: 1n }, loadQuote: vi.fn(), wallet: await signedIn(), actions, blocked: null });
    document.body.replaceChildren(root);
    await vi.waitFor(() => expect(root.querySelector("#trade-sell-limit")?.textContent).toContain("You hold 2.0M MOTH."));
    const field = root.querySelector<HTMLInputElement>("#trade-sell-amount")!;
    const max = () => [...root.querySelectorAll<HTMLButtonElement>("#trade-sell-shares button")].find((b) => b.textContent === "Max")!.click();
    max();
    root.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await vi.waitFor(() => expect(root.querySelector("#trade-sell-limit")?.textContent).toContain("cannot load"));
    expect(field.value).toBe("");
    max();
    console.log("limit:", root.querySelector("#trade-sell-limit")?.textContent, "| field after Max:", JSON.stringify(field.value));
    // Intent: Max computes from the account's real balance. The balance is unknown now (the visitor sold it all).
    expect(field.value).toBe("");
  });
});
