// @vitest-environment jsdom
// Review repros for the sell share buttons, kept as regression tests.
import { describe, expect, it, vi } from "vitest";
import { parseBuyQuote } from "../../src/buy-quote.ts";
import { drawTradeBox, type TradeActions } from "../../src/render-trade-box.ts";
import type { SellLimit, TradeOutcome } from "../../src/trade-actions.ts";
import { walletControl, type WalletControl } from "../../src/wallet-session.ts";
import { clockTimeWithZone } from "../../src/format.ts";

const PAGE = "https://example.test/?network=testnet&coin=moth.factory.testnet";
async function signedIn(): Promise<WalletControl> {
  const c = walletControl(PAGE, () => ({
    connect: async () => ({ accountId: "you.testnet", network: "testnet" }),
    restore: async () => null, send: async () => [], signOut: async () => {}, onSignIn: () => {}, onSignOut: () => {},
  }));
  await c.connect();
  return c;
}
function mk(balance: bigint | "pending" | "fail", sell: TradeActions["sell"] = async () => ({ kind: "not-sent", text: "x" })) {
  const actions: TradeActions = {
    buy: vi.fn(async (): Promise<TradeOutcome> => ({ kind: "not-sent", text: "x" })),
    sell: vi.fn(sell),
    sellLimit: vi.fn((): Promise<SellLimit> => balance === "pending" ? new Promise<SellLimit>(() => {}) : balance === "fail" ? Promise.reject(new Error("boom")) : Promise.resolve({ balance, canSellNow: balance, moreOpens: null })),
    onTraded: () => {},
  };
  return actions;
}
async function draw(actions: TradeActions) {
  const root = drawTradeBox({ symbol: "MOTH", decimals: 24, largeThresholdBps: 200, storageMin: { kind: "value", value: 1n }, loadQuote: async () => ({ kind: "value", value: parseBuyQuote({ tokens_out: "1", fee_near: "0", creator_fee_near: "0", marked_tokens: "0", class_after: "small", becomes_large: false, allowance_after: "0" }) }), wallet: await signedIn(), actions, blocked: null });
  document.body.replaceChildren(root);
  await new Promise((r) => setTimeout(r, 20));
  const $ = <T extends HTMLElement>(s: string) => root.querySelector<T>(s)!;
  const pick = (l: string) => [...root.querySelectorAll<HTMLButtonElement>("#trade-sell-shares button")].find((b) => b.textContent === l)!.click();
  return { root, $, pick };
}

describe("sell shares", () => {
  it("balance of 1 unit: 25% gives a visible reason, not a silent empty field", async () => {
    const { $, pick } = await draw(mk(1n));
    pick("25%");
    expect($<HTMLInputElement>("#trade-sell-amount").value === "" ? $("#trade-sell-status").textContent : "has value").not.toBe("");
  });
  it("Max with balance 1 unit is exact", async () => {
    const { $, pick } = await draw(mk(1n));
    pick("Max");
    expect($<HTMLInputElement>("#trade-sell-amount").value).toBe("0.000000000000000000000001");
  });
  it("balance above 2^53 never exceeds balance for any pct", async () => {
    const bal = 9007199254740993123456789012n;
    const { $, pick } = await draw(mk(bal));
    for (const p of ["25%", "50%", "75%", "Max"]) {
      pick(p);
      const [w, f = ""] = $<HTMLInputElement>("#trade-sell-amount").value.split(".");
      const u = BigInt(w!) * 10n ** 24n + BigInt(f.padEnd(24, "0"));
      expect(u <= bal).toBe(true);
    }
  });
  it("while the balance is still loading, Max does not say 'Connect a wallet' for a signed-in visitor", async () => {
    const { $, pick } = await draw(mk("pending"));
    pick("Max");
    expect($("#trade-sell-status").textContent).not.toContain("Connect a wallet");
  });
  it("when the balance read failed, Max does not say 'Connect a wallet' for a signed-in visitor", async () => {
    const { $, pick } = await draw(mk("fail"));
    pick("Max");
    expect($("#trade-sell-status").textContent).not.toContain("Connect a wallet");
  });
  it("an amount typed while a sell is pending is not wiped by the earlier sell's result", async () => {
    let done!: (o: TradeOutcome) => void;
    const a = mk(10n ** 24n * 10n, () => new Promise((r) => (done = r)));
    const { $, root } = await draw(a);
    $<HTMLInputElement>("#trade-sell-amount").value = "3";
    root.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await new Promise((r) => setTimeout(r, 5));
    $<HTMLInputElement>("#trade-sell-amount").value = "4";
    $<HTMLInputElement>("#trade-sell-amount").dispatchEvent(new Event("input"));
    done({ kind: "result", result: { kind: "sold", txHash: "h", explorerUrl: null, account: "you.testnet", fill: "full", tokensAsked: 3n, tokensSold: 3n, tokensKept: 0n, nearOut: 1n, penaltyNear: 0n, limitReason: null } as never });
    await new Promise((r) => setTimeout(r, 20));
    expect($<HTMLInputElement>("#trade-sell-amount").value).toBe("4");
  });
  it("after an unknown result the field is cleared so a second click cannot resend", async () => {
    const a = mk(10n ** 24n * 10n, async () => ({ kind: "unknown", text: "gone" }));
    const { $, root } = await draw(a);
    $<HTMLInputElement>("#trade-sell-amount").value = "3";
    const submit = () => root.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    submit();
    await new Promise((r) => setTimeout(r, 20));
    submit();
    await new Promise((r) => setTimeout(r, 20));
    expect(a.sell).toHaveBeenCalledTimes(1);
  });
  it("comma refused", async () => {
    const a = mk(10n ** 25n);
    const { $, root } = await draw(a);
    $<HTMLInputElement>("#trade-sell-amount").value = "1,5";
    root.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    expect($("#trade-sell-status").textContent).toBe("Use a dot for decimals.");
    expect(a.sell).not.toHaveBeenCalled();
  });
});

describe("zone", () => {
  it("labels", () => {
    const ns = BigInt(Date.UTC(2026, 6, 1, 12)) * 1_000_000n;
    const out = ["UTC", "Europe/London", "Asia/Kolkata", "America/St_Johns", "Asia/Kathmandu", "America/New_York"].map((timeZone) => clockTimeWithZone(ns, { timeZone }));
    expect(out[0]).toBe("12:00 UTC");
  });
});
