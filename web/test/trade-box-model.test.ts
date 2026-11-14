import { describe, expect, it } from "vitest";
import { parseBuyQuote } from "../src/buy-quote.ts";
import { tradeBoxModel } from "../src/trade-box-model.ts";
import { allText, copyViolations } from "./copy-rules.ts";

const quoteView = {
  tokens_out: "5600000000000000000000000",
  fee_near: "20000000000000000000000",
  creator_fee_near: "6000000000000000000000",
  marked_tokens: "0",
  class_after: "small",
  becomes_large: false,
  allowance_after: "7100000000000000000000000",
};
const base = { symbol: "MOTH", decimals: 18, largeThresholdBps: 200, storageMin: { kind: "value" as const, value: 1_250_000_000_000_000_000_000n }, signedIn: null, pending: false, blocked: null };
const asked = (over: object = {}) => ({
  ...base,
  preview: { kind: "quote" as const, accountId: "you.testnet", nearIn: 10n ** 24n, quote: { kind: "value" as const, value: parseBuyQuote({ ...quoteView, ...over }) } },
});

describe("trade box buttons", () => {
  it("asks for a wallet before a buy or a sell", () => {
    const m = tradeBoxModel({ ...base, preview: { kind: "none" } });
    expect(m.connect.label).toBe("Connect wallet");
    expect(m.buy.button.reason).toBe("Connect a wallet to buy.");
    expect(m.sell.button.reason).toBe("Connect a wallet to sell.");
  });

  it("turns the buy on for a signed-in account with a quote for that account", () => {
    const m = tradeBoxModel({ ...asked(), signedIn: "you.testnet" });
    expect(m.buy.button).toEqual({ label: "Buy 5.6M MOTH", reason: null });
    expect(m.sell.button.reason).toBeNull();
  });

  it("keeps the buy off while the quote is for another account", () => {
    const m = tradeBoxModel({ ...asked(), signedIn: "lamp.testnet" });
    expect(m.buy.button.reason).toBe("Preview the buy for lamp.testnet first.");
    expect(tradeBoxModel({ ...base, signedIn: "lamp.testnet", preview: { kind: "none" } }).buy.button.reason).toBe("Preview the buy for lamp.testnet first.");
  });

  it("turns both buttons off while the wallet is open", () => {
    const m = tradeBoxModel({ ...asked(), signedIn: "you.testnet", pending: true });
    for (const b of [m.buy.button, m.sell.button]) expect(b).toEqual({ label: "Waiting for the wallet", reason: "Approve or close the transaction in your wallet." });
  });
});

describe("a coin that fails the page's checks", () => {
  it("keeps buy and sell off with the warning, signed in or not", () => {
    const w = "Warning: this account does not match the Fair Chance launch rules.";
    const m = tradeBoxModel({ ...asked(), signedIn: "you.testnet", blocked: w });
    expect(m.buy.button.reason).toBe(w);
    expect(m.sell.button.reason).toBe(w);
  });
});

describe("the storage note", () => {
  it("says, from the coin's storage bounds, what a buy from an account that never held the coin also pays", () => {
    const m = tradeBoxModel({ ...base, preview: { kind: "none" } });
    expect(m.buy.storageNote).toBe("If this account never held this coin, the buy also pays a storage deposit of 0.00125 NEAR.");
    const other = tradeBoxModel({ ...base, storageMin: { kind: "value", value: 5n * 10n ** 21n }, preview: { kind: "none" } });
    expect(other.buy.storageNote).toContain("a storage deposit of 0.005 NEAR.");
  });

  it("says the amount is not available when the coin has no storage bounds", () => {
    const m = tradeBoxModel({ ...base, storageMin: { kind: "unavailable" }, preview: { kind: "none" } });
    expect(m.buy.storageNote).toBe("If this account never held this coin, the buy also pays a storage deposit. The amount is not available from this coin.");
  });

  it("shows no quote lines before a preview is asked for", () => {
    const m = tradeBoxModel({ ...base, preview: { kind: "none" } });
    expect(m.buy.lines).toEqual([]);
    expect(m.buy.largeWarning).toBeNull();
  });
});

describe("buy preview", () => {
  it("shows what the view gives, and the amount in the button label", () => {
    const m = tradeBoxModel(asked());
    expect(m.buy.lines).toContainEqual({ label: "You get about", value: "5.6M MOTH" });
    expect(m.buy.lines).toContainEqual({ label: "Swap fee, kept in the pool and creator fees", value: "0.020 NEAR" });
    expect(m.buy.lines).toContainEqual({ label: "Of that, to the creator fee balance", value: "0.006 NEAR" });
    expect(m.buy.lines).toContainEqual({ label: "Allowance after this buy", value: "7.1M MOTH per hour" });
    expect(m.buy.button.label).toBe("Buy 5.6M MOTH");
  });

  it("shows the large-buy warning when, and only when, the view says becomes_large", () => {
    expect(tradeBoxModel(asked({ becomes_large: false })).buy.largeWarning).toBeNull();
    const warn = tradeBoxModel(asked({ becomes_large: true, class_after: "large" })).buy.largeWarning;
    expect(warn).toMatch(/large holder/);
    expect(warn).toMatch(/2\.0% of supply/);
    expect(warn).toMatch(/cannot send tokens/);
  });

  it("does not compute becomes_large itself", () => {
    // A huge buy that the view says is not large shows no warning; a tiny one that it says is large does.
    const huge = tradeBoxModel({ ...asked(), preview: { kind: "quote", accountId: "a.testnet", nearIn: 10n ** 30n, quote: { kind: "value", value: parseBuyQuote(quoteView) } } });
    expect(huge.buy.largeWarning).toBeNull();
  });

  it("says so when the coin has no buy quote view", () => {
    const m = tradeBoxModel({ ...base, preview: { kind: "quote", accountId: "a.testnet", nearIn: 1n, quote: { kind: "unavailable" } } });
    expect(m.buy.lines).toEqual([]);
    expect(m.buy.note).toBe("Not available from this coin");
  });

  it("follows the copy rules", () => {
    for (const m of [tradeBoxModel({ ...base, preview: { kind: "none" } }), tradeBoxModel(asked({ becomes_large: true })), tradeBoxModel({ ...asked(), signedIn: "you.testnet", pending: true })]) {
      for (const t of allText(m)) expect(copyViolations(t), t).toEqual([]);
    }
  });
});

describe("warning source", () => {
  it("shows no warning for an account that is already large when the view says becomes_large is false", () => {
    const m = tradeBoxModel(asked({ class_after: "large", becomes_large: false }));
    expect(m.buy.largeWarning).toBeNull();
  });
});
