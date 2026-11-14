// @vitest-environment jsdom
// Review finding, kept as a test: a sessionStorage launch value stays on the coin page as its header when the chain reads fail.
import { describe, expect, it, vi } from "vitest";
import { readerFrom, recordedCoin } from "../recorded-coin.ts";

const coin = "ghost.factory.fairchance.testnet";
const rec = recordedCoin("rpc-error");
vi.mock("../../src/coin-page.ts", async (orig) => {
  const real = await orig<typeof import("../../src/coin-page.ts")>();
  return { ...real, loadCoinPage: (href: string, o: object) => real.loadCoinPage(href, { ...o, reader: readerFrom(rec, coin), nowNs: 1n }) };
});
vi.mock("../../src/page-wallet.ts", () => ({ pageWallet: () => ({ state: () => ({ kind: "signed-out" }), subscribe: () => () => {} }), mountWalletBar: () => {} }));

describe("launch handoff", () => {
  it("does not keep a header the chain never confirmed", async () => {
    sessionStorage.setItem(`fc-launched:${coin}`, JSON.stringify({ coin, name: "Official NEAR Token", ticker: "NEAR", icon: null, creator: "near", launchBlock: 1, explorerUrl: "https://example.com/phish" }));
    document.body.innerHTML = '<div id="app"></div>';
    history.replaceState(null, "", `/?network=testnet&coin=${coin}`);
    await import("../../src/render-coin-page.ts");
    await vi.waitFor(() => expect(document.querySelector("#load-error")).not.toBeNull(), { timeout: 5000 });
    expect(document.querySelector("#just-launched")).toBeNull();
  });
});
