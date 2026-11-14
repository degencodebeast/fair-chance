// @vitest-environment jsdom
// Review finding: a coin that failed the page's own chain checks still got a live trade box, and a sell went to it.
import { describe, expect, it, vi } from "vitest";
import { readerFrom, recordedCoin } from "../recorded-coin.ts";
import { walletControl } from "../../src/wallet-session.ts";

const coin = "moth.factory.testnet";
const rec = recordedCoin("normal");
const sent: unknown[] = [];

vi.mock("../../src/coin-page.ts", async (orig) => {
  const real = await orig<typeof import("../../src/coin-page.ts")>();
  return { ...real, loadCoinPage: (href: string, o: object) => real.loadCoinPage(href, { ...o, reader: readerFrom(rec, coin), nowNs: BigInt(rec.now_ns) }) };
});
vi.mock("../../src/chain.ts", async (orig) => {
  const real = await orig<typeof import("../../src/chain.ts")>();
  return {
    ...real,
    chainReader: () => ({
      ...readerFrom(rec, coin),
      view: async (m: string) => {
        if (m === "fc_quote_sell") return { tokens_filled: "1000", near_out: "5000", penalty_near: "0", fill: "full", limit_reason: null };
        if (m === "fc_account") return { balance: "1000" };
        return (rec.views as Record<string, unknown>)[m];
      },
    }),
  };
});
const wallet = walletControl("https://x.test/?network=testnet", () => ({
  connect: async () => ({ accountId: "you.testnet", network: "testnet" }),
  restore: async () => null,
  send: async (_a, txs) => (sent.push(txs), []),
  signOut: async () => {},
  onSignIn: () => {},
  onSignOut: () => {},
}));
vi.mock("../../src/page-wallet.ts", () => ({ pageWallet: () => wallet, mountWalletBar: () => {} }));

describe("coin that fails the page's checks", () => {
  it("turns buy and sell off with the warning and sends nothing to it", async () => {
    await wallet.connect();
    document.body.innerHTML = '<div id="app"></div>';
    history.replaceState(null, "", `/?network=testnet&coin=${coin}&rpc=${encodeURIComponent("http://127.0.0.1:1")}&factory=factory.testnet&code_hash=11111111111111111111111111111112`);
    await import("../../src/render-coin-page.ts");
    await vi.waitFor(() => expect(document.querySelector("#chain-warning")).not.toBeNull(), { timeout: 5000 });
    const warning = document.querySelector("#chain-warning b")!.textContent;
    const sellBtn = document.querySelector<HTMLButtonElement>("#trade-sell-button")!;
    expect(sellBtn.disabled).toBe(true);
    expect(document.querySelector("#trade-sell-button-reason")?.textContent).toBe(warning);
    expect(document.querySelector("#trade-buy-button-reason")?.textContent).toBe(warning);
    (document.querySelector<HTMLInputElement>("#trade-sell-amount")!).value = "0.000000000000001";
    document.querySelector<HTMLFormElement>("#trade-sell-form")!.dispatchEvent(new Event("submit", { cancelable: true }));
    await new Promise((r) => setTimeout(r, 200));
    expect(sent).toEqual([]);
  });
});
