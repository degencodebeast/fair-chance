// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { drawJustLaunched, saveLaunched, takeLaunched, type LaunchedCoin } from "../src/launch-handoff.ts";

const FACTORY = "factory.fairchance.testnet";
const HASH = "BgzcrdkorKb9iLzoS4a4WqVrTkL8ihXssb15wAmpw5RP";
const coin: LaunchedCoin = {
  coin: "moth.factory.fairchance.testnet",
  name: "Moth Lamp",
  ticker: "MOTH",
  icon: null,
  creator: "you.testnet",
  launchBlock: 277642845,
  txHash: HASH,
};

function memory(): Storage {
  const m = new Map<string, string>();
  return { getItem: (k) => m.get(k) ?? null, setItem: (k, v) => void m.set(k, v), removeItem: (k) => void m.delete(k), clear: () => m.clear(), key: () => null, get length() { return m.size; } };
}

describe("the launch result handed to the new coin page", () => {
  it("comes back once, for the same coin under the known factory, and is then removed", () => {
    const store = memory();
    saveLaunched(store, coin);
    expect(takeLaunched(store, "other.factory.fairchance.testnet", FACTORY)).toBeNull();
    expect(takeLaunched(store, coin.coin, FACTORY)).toEqual(coin);
    expect(takeLaunched(store, coin.coin, FACTORY)).toBeNull();
    expect(store.length).toBe(0);
  });

  it("is dropped when the coin is not <ticker>.<known factory>", () => {
    for (const [c, factory] of [
      [{ ...coin, coin: "moth.evil.testnet" }, FACTORY],
      [{ ...coin, coin: "x.moth.factory.fairchance.testnet", ticker: "X" }, FACTORY],
      [{ ...coin, ticker: "NEAR" }, FACTORY],
      [coin, null],
    ] as const) {
      const store = memory();
      saveLaunched(store, c);
      expect(takeLaunched(store, c.coin, factory)).toBeNull();
    }
  });

  it("is dropped when it is not the shape the page saved", () => {
    const store = memory();
    store.setItem(`fc-launched:${coin.coin}`, JSON.stringify({ ...coin, launchBlock: "x" }));
    expect(takeLaunched(store, coin.coin, FACTORY)).toBeNull();
    store.setItem(`fc-launched:${coin.coin}`, "not json");
    expect(takeLaunched(store, coin.coin, FACTORY)).toBeNull();
    store.setItem(`fc-launched:${coin.coin}`, JSON.stringify({ ...coin, name: "N".repeat(33) }));
    expect(takeLaunched(store, coin.coin, FACTORY)).toBeNull();
  });

  it("is null, not an error, when the browser keeps no storage", () => {
    const broken = { getItem: () => { throw new Error("blocked"); }, setItem: () => { throw new Error("blocked"); }, removeItem: () => {} } as unknown as Storage;
    expect(() => saveLaunched(broken, coin)).not.toThrow();
    expect(takeLaunched(broken, coin.coin, FACTORY)).toBeNull();
  });

  it("draws the coin at once: tile, name, ticker, creator, launch block and the known explorer link", () => {
    const node = drawJustLaunched(coin);
    expect(node.id).toBe("just-launched");
    expect(node.querySelector(".coin-tile")?.textContent).toBe("MO");
    expect(node.querySelector("h1")?.textContent).toBe("Moth Lamp");
    expect(node.textContent).toContain("MOTH");
    expect(node.textContent).toContain("Creator you.testnet");
    expect(node.textContent).toContain("Launched at block 277642845");
    expect(node.querySelector<HTMLAnchorElement>("a")?.href).toBe(`https://testnet.nearblocks.io/txns/${HASH}`);
  });

  it("draws no link for a hash that is not a transaction hash", () => {
    expect(drawJustLaunched({ ...coin, txHash: "https://example.com/phish" }).querySelector("a")).toBeNull();
  });
});
