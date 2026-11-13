// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { parseBuyQuote } from "../src/buy-quote.ts";
import { drawTradeBox } from "../src/render-trade-box.ts";
import { expectNoEnabledControlWithoutHandler, turnedOffReasons } from "./dom-controls.ts";

const quoteView = {
  tokens_out: "5600000000000000000000000",
  fee_near: "20000000000000000000000",
  creator_fee_near: "6000000000000000000000",
  marked_tokens: "0",
  class_after: "small",
  becomes_large: false,
  allowance_after: "7100000000000000000000000",
};

function box(over: object = {}) {
  const loadQuote = vi.fn(async () => ({ kind: "value" as const, value: parseBuyQuote({ ...quoteView, ...over }) }));
  const root = drawTradeBox({ symbol: "MOTH", decimals: 18, largeThresholdBps: 200, loadQuote });
  document.body.replaceChildren(root);
  return { root, loadQuote };
}

async function preview(root: HTMLElement, account: string, near: string) {
  root.querySelector<HTMLInputElement>("#trade-buy-account")!.value = account;
  root.querySelector<HTMLInputElement>("#trade-buy-amount")!.value = near;
  root.querySelector<HTMLFormElement>("#trade-buy-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
  await vi.waitFor(() => expect(root.querySelector("#trade-buy-status")?.textContent).not.toBe("Reading the chain."));
}

describe("trade box", () => {
  it("has no enabled control without a handler", () => {
    const { root } = box();
    expectNoEnabledControlWithoutHandler(root);
  });

  it("turns off connect, buy and sell, and each says why", () => {
    const { root } = box();
    const reasons = turnedOffReasons(root);
    expect(Object.keys(reasons).sort()).toEqual(["trade-buy-button", "trade-connect", "trade-sell-button"]);
    expect(reasons["trade-buy-button"]).toMatch(/storage deposit/);
    expect(reasons["trade-connect"]).toMatch(/turned off/);
  });

  it("switches between the buy and sell tabs", () => {
    const { root } = box();
    const tab = (id: string) => root.querySelector<HTMLButtonElement>(id)!;
    expect(tab("#trade-tab-buy").getAttribute("aria-selected")).toBe("true");
    expect(root.querySelector<HTMLElement>("#trade-sell-panel")!.hidden).toBe(true);
    tab("#trade-tab-sell").click();
    expect(tab("#trade-tab-sell").getAttribute("aria-selected")).toBe("true");
    expect(root.querySelector<HTMLElement>("#trade-sell-panel")!.hidden).toBe(false);
    expect(root.querySelector<HTMLElement>("#trade-buy-panel")!.hidden).toBe(true);
    expectNoEnabledControlWithoutHandler(root);
  });

  it("asks the coin for the quote of the typed account and amount, in yoctoNEAR", async () => {
    const { root, loadQuote } = box();
    await preview(root, "you.testnet", "1.5");
    expect(loadQuote).toHaveBeenCalledWith("you.testnet", 1_500_000_000_000_000_000_000_000n);
    expect(root.querySelector("#trade-buy-lines")?.textContent).toContain("5.6M MOTH");
    expect(root.querySelector("#trade-buy-button")!.textContent).toBe("Buy 5.6M MOTH");
    expect((root.querySelector("#trade-buy-button") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows the large-buy warning only when the quote says becomes_large", async () => {
    const quiet = box({ becomes_large: false });
    await preview(quiet.root, "you.testnet", "1");
    expect(quiet.root.querySelector("#large-buy-warning")).toBeNull();
    const loud = box({ becomes_large: true, class_after: "large" });
    await preview(loud.root, "you.testnet", "1");
    const warn = loud.root.querySelector("#large-buy-warning")!;
    expect(warn.getAttribute("role")).toBe("alert");
    expect(warn.textContent).toMatch(/large holder/);
    expectNoEnabledControlWithoutHandler(loud.root);
  });

  it("asks for an account and a NEAR amount before it reads", async () => {
    const { root, loadQuote } = box();
    await preview(root, "", "1");
    expect(root.querySelector("#trade-buy-status")?.textContent).toMatch(/account name/);
    await preview(root, "you.testnet", "abc");
    expect(root.querySelector("#trade-buy-status")?.textContent).toMatch(/amount of NEAR/);
    expect(loadQuote).not.toHaveBeenCalled();
  });

  it("shows an error and no number when the read fails", async () => {
    const root = drawTradeBox({ symbol: "MOTH", decimals: 18, largeThresholdBps: 200, loadQuote: async () => { throw new Error("rpc down"); } });
    document.body.replaceChildren(root);
    await preview(root, "you.testnet", "1");
    expect(root.querySelector("#trade-buy-status")?.textContent).toMatch(/cannot load/);
    expect(root.querySelector("#trade-buy-lines")?.textContent ?? "").toBe("");
  });
});

describe("trade box with slow answers", () => {
  it("drops an older quote that arrives after a newer one", async () => {
    const pending: { resolve: (v: { kind: "value"; value: ReturnType<typeof parseBuyQuote> }) => void }[] = [];
    const root = drawTradeBox({
      symbol: "MOTH", decimals: 18, largeThresholdBps: 200,
      loadQuote: () => new Promise((resolve) => { pending.push({ resolve }); }),
    });
    document.body.replaceChildren(root);
    const submit = (near: string) => {
      root.querySelector<HTMLInputElement>("#trade-buy-account")!.value = "you.testnet";
      root.querySelector<HTMLInputElement>("#trade-buy-amount")!.value = near;
      root.querySelector<HTMLFormElement>("#trade-buy-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    };
    submit("100");
    submit("1");
    const answer = (becomesLarge: boolean, tokens: string) => ({ kind: "value" as const, value: parseBuyQuote({ ...quoteView, becomes_large: becomesLarge, tokens_out: tokens }) });
    pending[1]!.resolve(answer(false, "1000000000000000000000000"));
    await vi.waitFor(() => expect(root.querySelector("#trade-buy-lines")?.textContent).toContain("1.0M"));
    pending[0]!.resolve(answer(true, "90000000000000000000000000"));
    await new Promise((r) => setTimeout(r, 20));
    expect(root.querySelector("#trade-buy-lines")?.textContent).toContain("1.0M");
    expect(root.querySelector("#trade-buy-lines")?.textContent).not.toContain("90.0M");
    expect(root.querySelector("#large-buy-warning")).toBeNull();
  });
});
