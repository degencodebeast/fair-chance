// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { screenHeader, setHeaderWallet } from "../src/screen-chrome.ts";
import { lazyLibrary, mountWalletBar } from "../src/page-wallet.ts";
import { walletControl, type WalletAccount, type WalletLibrary } from "../src/wallet-session.ts";

const PAGE = "https://example.test/?network=testnet";
const flush = () => new Promise((r) => setTimeout(r, 0));

function fakeLibrary(saved: WalletAccount | null = null) {
  const fired: { signIn?: (a: WalletAccount) => void; signOut?: () => void } = {};
  const lib: WalletLibrary = {
    connect: async () => ({ accountId: "lamp.testnet", network: "testnet" }),
    restore: async () => saved,
    send: async () => [],
    signOut: async () => {},
    onSignIn: (l) => void (fired.signIn = l),
    onSignOut: (l) => void (fired.signOut = l),
  };
  return { lib, fired };
}

describe("the lazy wallet library", () => {
  it("loads the wallet code only on first use, and only once", async () => {
    let loads = 0;
    const { lib } = fakeLibrary();
    const lazy = lazyLibrary(async () => (loads++, lib));
    expect(loads).toBe(0);
    await lazy.connect();
    await lazy.restore();
    expect(loads).toBe(1);
  });

  it("passes wallet sign-in and sign-out events through after the code loads", async () => {
    const { lib, fired } = fakeLibrary();
    const lazy = lazyLibrary(async () => lib);
    const seen: string[] = [];
    lazy.onSignIn((a) => seen.push(`in ${a.accountId}`));
    lazy.onSignOut(() => seen.push("out"));
    await lazy.restore();
    fired.signIn!({ accountId: "wick.testnet", network: "testnet" });
    fired.signOut!();
    expect(seen).toEqual(["in wick.testnet", "out"]);
  });

  it("passes the account to send and sign out, so they reach the wallet that gave it", async () => {
    const seen: unknown[] = [];
    const { lib } = fakeLibrary();
    const lazy = lazyLibrary(async () => ({ ...lib, send: async (a, txs) => (seen.push(["send", a, txs]), []), signOut: async (a) => void seen.push(["signOut", a]) }));
    const account = { accountId: "lamp.testnet", network: "testnet", wallet: "w1" };
    await lazy.send(account, []);
    await lazy.signOut(account);
    expect(seen).toEqual([["send", account, []], ["signOut", account]]);
  });

  it("rejects each call when the wallet code cannot load", async () => {
    const lazy = lazyLibrary(async () => {
      throw new Error("chunk failed");
    });
    await expect(lazy.connect()).rejects.toThrow("chunk failed");
  });
});

describe("the wallet bar on every screen", () => {
  const COINS = "https://example.test/?network=testnet&view=coins";
  afterEach(() => setHeaderWallet(null));

  it("sits on the right of each app screen header, and a redraw keeps the one bar", () => {
    document.body.innerHTML = '<div id="network-banner"></div><main id="app"></main>';
    const control = walletControl(PAGE, () => fakeLibrary().lib);
    mountWalletBar(document, control, COINS);
    const app = document.querySelector("#app")!;
    app.replaceChildren(screenHeader(COINS, "coins"));
    const first = document.querySelector("#wallet-bar");
    expect(first?.parentElement?.className).toBe("screen-header");
    expect(document.querySelector("#header-open-app")).toBeNull();
    expect(first?.textContent).toBe("Connect wallet");
    app.replaceChildren(screenHeader(COINS, "coins"));
    expect(document.querySelectorAll("#wallet-bar")).toHaveLength(1);
    expect(document.querySelector("#wallet-bar")).toBe(first);
    mountWalletBar(document, control, COINS);
    expect(document.querySelectorAll("#wallet-bar")).toHaveLength(1);
  });

  it("keeps Open app on the landing header and puts no wallet bar there", () => {
    document.body.innerHTML = '<main id="app"></main>';
    mountWalletBar(document, walletControl(PAGE, () => fakeLibrary().lib), PAGE);
    document.querySelector("#app")!.replaceChildren(screenHeader(PAGE, "landing"));
    expect(document.querySelector("#header-open-app")?.textContent).toBe("Open app");
    expect(document.querySelector("#wallet-bar")).toBeNull();
  });

  it("gives the coin page a header before the app, so a redraw of the coin page keeps it", () => {
    const COIN = "https://example.test/?network=testnet&coin=bag.factory.fairchance.testnet";
    document.body.innerHTML = '<main id="app"></main>';
    mountWalletBar(document, walletControl(PAGE, () => fakeLibrary().lib), COIN);
    document.querySelector("#app")!.replaceChildren();
    expect(document.querySelector("header.screen-header")?.nextElementSibling?.id).toBe("app");
    expect(document.querySelector("header.screen-header #wallet-bar")).not.toBeNull();
  });

  it("shows a session saved by an earlier visit after restore", async () => {
    document.body.innerHTML = '<main id="app"></main>';
    const control = walletControl(PAGE, () => fakeLibrary({ accountId: "lamp.testnet", network: "testnet" }).lib);
    mountWalletBar(document, control, COINS);
    document.querySelector("#app")!.replaceChildren(screenHeader(COINS, "coins"));
    await control.restore();
    await flush();
    expect(document.querySelector(".screen-header #wallet-account")?.textContent).toBe("lamp.testnet");
  });
});
