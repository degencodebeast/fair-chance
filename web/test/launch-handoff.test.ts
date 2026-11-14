// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { drawJustLaunched, saveLaunched, takeLaunched, type LaunchedCoin } from "../src/launch-handoff.ts";

const coin: LaunchedCoin = {
  coin: "moth.factory.fairchance.testnet",
  name: "Moth Lamp",
  ticker: "MOTH",
  icon: null,
  creator: "you.testnet",
  launchBlock: 277642845,
  explorerUrl: "https://testnet.nearblocks.io/txns/Hx9",
};

function memory(): Storage {
  const m = new Map<string, string>();
  return { getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v), removeItem: (k) => void m.delete(k), clear: () => m.clear(), key: () => null, length: 0 };
}

describe("the launch result handed to the new coin page", () => {
  it("comes back for the same coin only", () => {
    const store = memory();
    saveLaunched(store, coin);
    expect(takeLaunched(store, "other.factory.fairchance.testnet")).toBeNull();
    expect(takeLaunched(store, coin.coin)).toEqual(coin);
  });

  it("is dropped when it is not the shape the page saved", () => {
    const store = memory();
    store.setItem(`fc-launched:${coin.coin}`, JSON.stringify({ ...coin, launchBlock: "x" }));
    expect(takeLaunched(store, coin.coin)).toBeNull();
    store.setItem(`fc-launched:${coin.coin}`, "not json");
    expect(takeLaunched(store, coin.coin)).toBeNull();
  });

  it("is null, not an error, when the browser keeps no storage", () => {
    const broken = { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("blocked"); } } as unknown as Storage;
    expect(() => saveLaunched(broken, coin)).not.toThrow();
    expect(takeLaunched(broken, coin.coin)).toBeNull();
  });

  it("draws the coin at once: tile, name, ticker, creator and launch block", () => {
    const node = drawJustLaunched(coin);
    expect(node.id).toBe("just-launched");
    expect(node.querySelector(".coin-tile")?.textContent).toBe("MO");
    expect(node.querySelector("h1")?.textContent).toBe("Moth Lamp");
    expect(node.textContent).toContain("MOTH");
    expect(node.textContent).toContain("Creator you.testnet");
    expect(node.textContent).toContain("Launched at block 277642845");
    expect(node.querySelector<HTMLAnchorElement>("a")?.href).toBe(coin.explorerUrl);
  });
});
