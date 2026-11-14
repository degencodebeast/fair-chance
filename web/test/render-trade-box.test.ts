// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { parseBuyQuote } from "../src/buy-quote.ts";
import { drawTradeBox, type TradeActions } from "../src/render-trade-box.ts";
import type { TradeOutcome } from "../src/trade-actions.ts";
import { walletControl, type WalletControl } from "../src/wallet-session.ts";
import { expectNoEnabledControlWithoutHandler, turnedOffReasons } from "./dom-controls.ts";

const PAGE = "https://example.test/?network=testnet&coin=moth.factory.testnet";
let connects = 0;
function signedOut(): WalletControl {
  return walletControl(PAGE, () => ({
    connect: async () => (connects++, { accountId: "you.testnet", network: "testnet" }),
    restore: async () => null,
    send: async () => [],
    signOut: async () => {},
    onSignIn: () => {},
    onSignOut: () => {},
  }));
}
async function signedIn(): Promise<WalletControl> {
  const c = signedOut();
  await c.connect();
  return c;
}
function noActions(over: Partial<TradeActions> = {}): TradeActions & { traded: number } {
  const a = {
    traded: 0,
    buy: vi.fn(async (): Promise<TradeOutcome> => ({ kind: "not-sent", text: "x" })),
    sell: vi.fn(async (): Promise<TradeOutcome> => ({ kind: "not-sent", text: "x" })),
    sellLimit: vi.fn(async () => ({ balance: 2n * 10n ** 24n, canSellNow: 10n ** 24n })),
    onTraded: () => void a.traded++,
    ...over,
  };
  return a;
}

const quoteView = {
  tokens_out: "5600000000000000000000000",
  fee_near: "20000000000000000000000",
  creator_fee_near: "6000000000000000000000",
  marked_tokens: "0",
  class_after: "small",
  becomes_large: false,
  allowance_after: "7100000000000000000000000",
};

function box(over: object = {}, wallet: WalletControl = signedOut(), actions: TradeActions = noActions(), blocked: string | null = null) {
  const loadQuote = vi.fn(async () => ({ kind: "value" as const, value: parseBuyQuote({ ...quoteView, ...over }) }));
  const root = drawTradeBox({ symbol: "MOTH", decimals: 18, largeThresholdBps: 200, storageMin: { kind: "value", value: 1_250_000_000_000_000_000_000n }, loadQuote, wallet, actions, blocked });
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

  it("turns off buy and sell until a wallet connects, and each says why", () => {
    const { root } = box();
    const reasons = turnedOffReasons(root);
    expect(Object.keys(reasons).sort()).toEqual(["trade-buy-button", "trade-sell-button"]);
    expect(reasons["trade-buy-button"]).toBe("Connect a wallet to buy.");
    expect(root.querySelector("#trade-buy-storage")?.textContent).toBe("If this account never held this coin, the buy also pays a storage deposit of 0.00125 NEAR.");
  });

  it("opens the wallet from its connect button", async () => {
    const { root } = box();
    const before = connects;
    root.querySelector<HTMLButtonElement>("#trade-connect")!.click();
    await vi.waitFor(() => expect(connects).toBe(before + 1));
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
    const root = drawTradeBox({ symbol: "MOTH", decimals: 18, largeThresholdBps: 200, storageMin: { kind: "unavailable" }, loadQuote: async () => { throw new Error("rpc down"); }, wallet: signedOut(), actions: noActions(), blocked: null });
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
      symbol: "MOTH", decimals: 18, largeThresholdBps: 200, storageMin: { kind: "unavailable" },
      loadQuote: () => new Promise((resolve) => { pending.push({ resolve }); }),
      wallet: signedOut(),
      actions: noActions(),
      blocked: null,
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

const bought: TradeOutcome = {
  kind: "result",
  result: { kind: "bought", txHash: "Hx1", explorerUrl: "https://testnet.nearblocks.io/txns/Hx1", account: "you.testnet", nearIn: 10n ** 24n, tokensOut: 56n * 10n ** 23n, markedTokens: 0n, feeNear: 10n ** 22n },
};

describe("a buy from the trade box", () => {
  it("fills in the signed-in account and turns the buy on after its preview", async () => {
    const { root } = box({}, await signedIn());
    expect(root.querySelector<HTMLInputElement>("#trade-buy-account")!.value).toBe("you.testnet");
    expect(root.querySelector("#trade-connect")).toBeNull();
    await preview(root, "you.testnet", "1");
    expect(root.querySelector<HTMLButtonElement>("#trade-buy-button")!.disabled).toBe(false);
    expectNoEnabledControlWithoutHandler(root);
  });

  it("turns the button off while the wallet is open, then shows the result with the explorer link and reads the coin again", async () => {
    let answer!: (o: TradeOutcome) => void;
    const actions = noActions({ buy: vi.fn(() => new Promise<TradeOutcome>((r) => (answer = r))) });
    const { root } = box({}, await signedIn(), actions);
    await preview(root, "you.testnet", "1");
    root.querySelector<HTMLButtonElement>("#trade-buy-button")!.click();
    const button = () => root.querySelector<HTMLButtonElement>("#trade-buy-button")!;
    expect(button().disabled).toBe(true);
    expect(button().textContent).toBe("Waiting for the wallet");
    expect(actions.buy).toHaveBeenCalledWith(expect.objectContaining({ accountId: "you.testnet" }), { nearIn: 10n ** 24n, quote: parseBuyQuote(quoteView) });
    answer(bought);
    await vi.waitFor(() => expect(root.querySelector("#trade-result")?.textContent).toContain("You bought 5.6M MOTH for 1.0000 NEAR."));
    expect(root.querySelector<HTMLAnchorElement>("#trade-result a")!.href).toBe("https://testnet.nearblocks.io/txns/Hx1");
    expect(button().disabled).toBe(false);
    expect(actions.traded).toBe(1);
  });

  it("shows one sentence and turns the button back on when the wallet closes", async () => {
    const actions = noActions({ buy: vi.fn(async (): Promise<TradeOutcome> => ({ kind: "not-sent", text: "The wallet closed and sent nothing." })) });
    const { root } = box({}, await signedIn(), actions);
    await preview(root, "you.testnet", "1");
    root.querySelector<HTMLButtonElement>("#trade-buy-button")!.click();
    await vi.waitFor(() => expect(root.querySelector("#trade-result")?.textContent).toBe("The wallet closed and sent nothing."));
    expect(root.querySelector<HTMLButtonElement>("#trade-buy-button")!.disabled).toBe(false);
    expect(actions.traded).toBe(0);
  });
});

describe("a sell from the trade box", () => {
  it("says how much the account can sell now", async () => {
    const actions = noActions();
    const { root } = box({}, await signedIn(), actions);
    await vi.waitFor(() => expect(root.querySelector("#trade-sell-limit")?.textContent).toBe("You can sell up to 1.0M MOTH now. You hold 2.0M MOTH."));
    expect(actions.sellLimit).toHaveBeenCalledWith("you.testnet");
  });

  it("sends the typed amount in base units and shows the result", async () => {
    const sold: TradeOutcome = {
      kind: "result",
      result: { kind: "sold", txHash: "Hx2", explorerUrl: "https://testnet.nearblocks.io/txns/Hx2", account: "you.testnet", fill: "partial", tokensAsked: 15n * 10n ** 17n, tokensSold: 10n ** 18n, tokensKept: 5n * 10n ** 17n, nearOut: 10n ** 23n, penaltyNear: 0n, limitReason: "allowance" },
    };
    const actions = noActions({ sell: vi.fn(async () => sold) });
    const { root } = box({}, await signedIn(), actions);
    root.querySelector<HTMLButtonElement>("#trade-tab-sell")!.click();
    root.querySelector<HTMLInputElement>("#trade-sell-amount")!.value = "1.5";
    root.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await vi.waitFor(() => expect(root.querySelector("#trade-result")?.textContent).toContain("You sold 1.0 of 1.5 MOTH for 0.1000 NEAR."));
    expect(actions.sell).toHaveBeenCalledWith(expect.objectContaining({ accountId: "you.testnet" }), 15n * 10n ** 17n);
    expect(root.querySelector("#trade-result")?.textContent).toContain("This is all your allowance lets through.");
    expect(actions.traded).toBe(1);
    expectNoEnabledControlWithoutHandler(root);
  });

  it("asks for a number before it sends", async () => {
    const actions = noActions();
    const { root } = box({}, await signedIn(), actions);
    root.querySelector<HTMLInputElement>("#trade-sell-amount")!.value = "lots";
    root.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    expect(root.querySelector("#trade-sell-status")?.textContent).toBe("Type the amount of MOTH as a number, for example 1.5.");
    expect(actions.sell).not.toHaveBeenCalled();
  });
});

describe("the buy the visitor previewed", () => {
  const edit = (root: HTMLElement, id: string, value: string) => {
    const input = root.querySelector<HTMLInputElement>(id)!;
    input.value = value;
    input.dispatchEvent(new Event("input"));
  };

  for (const id of ["#trade-buy-amount", "#trade-buy-account"]) {
    it(`turns the buy off and drops the preview when ${id} changes`, async () => {
      const actions = noActions();
      const { root } = box({}, await signedIn(), actions);
      await preview(root, "you.testnet", "1");
      edit(root, id, id === "#trade-buy-amount" ? "0.1" : "you.testnet ");
      const button = root.querySelector<HTMLButtonElement>("#trade-buy-button")!;
      expect(button.disabled).toBe(true);
      expect(root.querySelector("#trade-buy-lines")?.textContent).toBe("");
      button.click();
      expect(actions.buy).not.toHaveBeenCalled();
    });
  }
});

describe("decimal commas", () => {
  it("refuses a comma in the buy amount", async () => {
    const { root, loadQuote } = box({}, await signedIn());
    await preview(root, "you.testnet", "1,5");
    expect(root.querySelector("#trade-buy-status")?.textContent).toBe("Use a dot for decimals.");
    expect(loadQuote).not.toHaveBeenCalled();
  });

  it("refuses a comma in the sell amount", async () => {
    const actions = noActions();
    const { root } = box({}, await signedIn(), actions);
    root.querySelector<HTMLInputElement>("#trade-sell-amount")!.value = "1,5";
    root.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    expect(root.querySelector("#trade-sell-status")?.textContent).toBe("Use a dot for decimals.");
    expect(actions.sell).not.toHaveBeenCalled();
  });
});

describe("a coin that fails the page's checks", () => {
  const WARNING = "Warning: this account does not match the Fair Chance launch rules.";

  it("turns buy and sell off with the warning, and sends nothing", async () => {
    const actions = noActions();
    const { root } = box({}, await signedIn(), actions, WARNING);
    await preview(root, "you.testnet", "1");
    const reasons = turnedOffReasons(root);
    expect(reasons["trade-buy-button"]).toBe(WARNING);
    expect(reasons["trade-sell-button"]).toBe(WARNING);
    root.querySelector<HTMLButtonElement>("#trade-buy-button")!.click();
    root.querySelector<HTMLInputElement>("#trade-sell-amount")!.value = "1";
    root.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await new Promise((r) => setTimeout(r, 10));
    expect(actions.buy).not.toHaveBeenCalled();
    expect(actions.sell).not.toHaveBeenCalled();
  });
});
