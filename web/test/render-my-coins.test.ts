// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import type { MyCoin, MyCoins } from "../src/my-coins.ts";
import { drawMyCoins } from "../src/render-my-coins.ts";
import { HttpStatusError } from "../src/refresher.ts";
import { walletControl, type WalletControl } from "../src/wallet-session.ts";
import { expectNoEnabledControlWithoutHandler } from "./dom-controls.ts";

const HREF = "https://example.test/?view=mine&network=testnet";
function wallet(): WalletControl {
  return walletControl(HREF, () => ({
    connect: async () => ({ accountId: "you.testnet", network: "testnet" }),
    restore: async () => null,
    send: async () => [],
    signOut: async () => {},
    onSignIn: () => {},
    onSignOut: () => {},
  }));
}
const coin = (over: Partial<MyCoin>): MyCoin => ({ coin: "a.factory.testnet", name: "Apple", ticker: "APL", icon: null, decimals: 18, launchBlock: 10, balance: 0n, canSellNow: 0n, ...over });
const mine: MyCoins = {
  launched: [coin({})],
  held: [coin({ coin: "b.factory.testnet", name: "Bee", ticker: "BEE", balance: 5n * 10n ** 18n, canSellNow: 2n * 10n ** 18n })],
};

function draw(w: WalletControl, load = vi.fn(async () => mine)) {
  const app = document.createElement("main");
  document.body.replaceChildren(app);
  drawMyCoins(app, HREF, { wallet: w, load });
  return { app, load };
}

describe("my coins screen", () => {
  it("asks for a wallet when none is connected, and reads nothing", () => {
    const { app, load } = draw(wallet());
    expect(app.querySelector("h1")?.textContent).toBe("My coins");
    expect(app.querySelector("#mine-status")?.textContent).toBe("Connect a wallet to see the coins you launched and hold.");
    expect(load).not.toHaveBeenCalled();
    expectNoEnabledControlWithoutHandler(app);
  });

  it("reads the coins after a sign-in and lists launched and held coins with what can sell now", async () => {
    const w = wallet();
    const { app, load } = draw(w);
    await w.connect();
    await vi.waitFor(() => expect(app.querySelectorAll("#mine-held li")).toHaveLength(1));
    expect(load).toHaveBeenCalledWith("you.testnet");
    const launched = app.querySelector("#mine-launched li")!;
    expect(launched.textContent).toContain("Apple");
    expect(launched.textContent).toContain("You hold no APL.");
    expect(launched.querySelector("a")?.getAttribute("href")).toBe("?coin=a.factory.testnet&network=testnet");
    const held = app.querySelector("#mine-held li")!;
    expect(held.querySelector(".coin-tile")?.textContent).toBe("BE");
    expect(held.textContent).toContain("You hold 5.0 BEE. You can sell up to 2.0 BEE now.");
  });

  it("says so when the account launched and holds nothing", async () => {
    const w = wallet();
    await w.connect();
    const { app } = draw(w, vi.fn(async () => ({ launched: [], held: [] })));
    await vi.waitFor(() => expect(app.querySelector("#mine-launched")?.textContent).toBe("You launched no coin yet."));
    expect(app.querySelector("#mine-held")?.textContent).toBe("You hold no coin from this factory.");
  });

  it("shows the error and no numbers when a read fails", async () => {
    const w = wallet();
    await w.connect();
    const { app } = draw(w, vi.fn(async () => { throw new Error("rpc down"); }));
    await vi.waitFor(() => expect(app.querySelector("#mine-status")?.textContent).toBe("Your coins cannot load from the chain. rpc down"));
    expect(app.querySelector("#mine-held")).toBeNull();
  });
});

describe("my coins screen after a failed read", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("shows a Try again button that reads the list again", async () => {
    const w = wallet();
    await w.connect();
    let n = 0;
    const { app, load } = draw(w, vi.fn(async () => {
      if (n++ === 0) throw new Error("rpc down");
      return mine;
    }));
    await vi.waitFor(() => expect(app.querySelector("#mine-retry")?.textContent).toBe("Try again"));
    expectNoEnabledControlWithoutHandler(app);
    app.querySelector<HTMLButtonElement>("#mine-retry")!.click();
    await vi.waitFor(() => expect(app.querySelectorAll("#mine-held li")).toHaveLength(1));
    expect(load).toHaveBeenCalledTimes(2);
    expect(app.querySelector("#mine-retry")).toBeNull();
  });

  it("keeps trying by itself after a rate limit or no readable answer, with the 4, 8 and 16 second backoff", async () => {
    vi.useFakeTimers();
    const w = wallet();
    await w.connect();
    const at: number[] = [];
    const start = Date.now();
    const { app } = draw(w, vi.fn(async () => {
      at.push(Date.now() - start);
      if (at.length === 1) throw new TypeError("Failed to fetch");
      if (at.length <= 4) throw new HttpStatusError("busy", 429, null);
      return mine;
    }));
    await vi.advanceTimersByTimeAsync(0);
    expect(app.querySelector("#mine-status")?.textContent).toBe("Your coins cannot load from the chain. Failed to fetch Trying again by itself.");
    expect(app.querySelector("#mine-retry")).not.toBeNull();
    await vi.advanceTimersByTimeAsync(4_000 + 8_000 + 16_000 + 32_000);
    expect(at).toEqual([0, 4_000, 12_000, 28_000, 60_000]);
    expect(app.querySelectorAll("#mine-held li")).toHaveLength(1);
  });

  it("does not try by itself after an error it does not know", async () => {
    vi.useFakeTimers();
    const w = wallet();
    await w.connect();
    const { app, load } = draw(w, vi.fn(async () => { throw new HttpStatusError("gone", 403, null); }));
    await vi.advanceTimersByTimeAsync(120_000);
    expect(load).toHaveBeenCalledTimes(1);
    expect(app.querySelector("#mine-status")?.textContent).toBe("Your coins cannot load from the chain. gone");
    expect(app.querySelector("#mine-retry")).not.toBeNull();
  });
});
