import { describe, expect, it } from "vitest";
import type { ChainReader, CoinEvent } from "../src/chain.ts";
import { loadCoinCard, loadCoinCards, MOST_COINS_SHOWN } from "../src/coin-cards-from-chain.ts";
import { readerFrom, recordedCoin } from "./recorded-coin.ts";
import { copyViolations } from "./copy-rules.ts";

const coin = "moth.factory.testnet";
const ctxFor = (state: string) => ({ nowNs: BigInt(recordedCoin(state).now_ns), timeZone: "UTC" });
const card = (state: string, patch?: (r: ReturnType<typeof recordedCoin>) => void) => {
  const rec = recordedCoin(state);
  patch?.(rec);
  return loadCoinCard(readerFrom(rec, coin), coin, ctxFor(state));
};

describe("a coin card read from the coin's views and events", () => {
  it("shows the normal coin", async () => {
    const c = await card("normal");
    expect(c).toMatchObject({
      account: coin, name: "Moth Lamp", ticker: "MOTH", age: "1 h 22 min", ageSeconds: 4920, marketCap: "172.99 NEAR", early: "18.4%",
      cap: { label: "Cap open, 0.90 of 2.13 NEAR out", tone: "open" },
      latest: { account: "alice.testnet", text: "sold 500K MOTH. It filled in full." },
      bigSellHeld: false, inEarlyWindow: false, lastSellAtNs: 1794579660000000000n, change: null, source: "chain",
    });
    expect(c.marketCapNear).toBeCloseTo(172.99, 2);
  });

  it("shows a full cap with the reset time and a held big sell", async () => {
    const c = await card("cap-full");
    expect(c.cap).toEqual({ label: "Cap full until 15:02", tone: "full" });
    expect(c.latest).toEqual({ account: "you.testnet", text: "tried to sell 30.0M MOTH. The rule filled 100K. Its allowance is used up until 15:02." });
    expect(c.bigSellHeld).toBe(true);
  });

  it("shows the early window with its blocks left and the latest early buy", async () => {
    const c = await card("early-window-live");
    expect(c.cap).toEqual({ label: "Early window, 12 blocks left", tone: "early" });
    expect(c.early).toBe("18.4% so far");
    expect(c.inEarlyWindow).toBe(true);
    expect(c.latest).toEqual({ account: "lamp.testnet", text: "bought 10.0M MOTH in block 1. These tokens count as early supply." });
  });

  it("says no sells yet for a coin with no sell or early buy", async () => {
    const c = await card("empty-receipts");
    expect(c.latest).toEqual({ account: "", text: "No sells yet." });
    expect(c.lastSellAtNs).toBeNull();
  });

  it("does not invent a market cap when the coin has no pool view", async () => {
    const c = await card("normal", (r) => r.missing_views.push("fc_pool"));
    expect(c.marketCap).toBe("Not available from this coin");
    expect(c.marketCapNear).toBeNull();
  });

  it("fails as a whole when a read fails", async () => {
    await expect(card("rpc-error")).rejects.toThrow();
  });

  it("follows the copy rules", async () => {
    for (const state of ["normal", "cap-full", "early-window-live", "empty-receipts"]) {
      const c = await card(state);
      for (const text of [c.cap.label, c.latest.text, c.early, c.marketCap]) expect(copyViolations(text), text).toEqual([]);
    }
  });
});

function launch(name: string, block: number): CoinEvent {
  return { standard: "fair_chance_factory", version: "1.0.0-rc.5", name: "fc_launch", coin: "factory.testnet", receiptId: `l${block}`, block, timeNs: null, data: { coin: name, creator: "c.testnet", launch_block: block } };
}
function factoryReader(events: CoinEvent[]): ChainReader {
  return { async view() { throw new Error("no view"); }, async account() { throw new Error("no"); }, async accessKeyCount() { throw new Error("no"); }, async events(since) { return { events: events.filter((e) => e.block >= (since ?? 0)), cursor: 0 }; } };
}

describe("the coin list read from the factory's launch events", () => {
  it("makes one card per launched coin, once each", async () => {
    const events = [launch("a.factory.testnet", 10), launch("b.factory.testnet", 20), launch("a.factory.testnet", 10)];
    const seen: string[] = [];
    const cards = await loadCoinCards(factoryReader(events), 0, (c) => { seen.push(c); return readerFrom(recordedCoin("normal"), c); }, ctxFor("normal"));
    expect(cards.map((c) => c.account).sort()).toEqual(["a.factory.testnet", "b.factory.testnet"]);
    expect(seen.sort()).toEqual(["a.factory.testnet", "b.factory.testnet"]);
  });

  it("ignores events that are not launches and gives no cards for none", async () => {
    const other = { ...launch("x.factory.testnet", 5), name: "fc_something" };
    expect(await loadCoinCards(factoryReader([other]), 0, () => { throw new Error("no coin"); }, ctxFor("normal"))).toEqual([]);
  });

  it("ignores a launch event that a coin contract emitted under the coin standard", async () => {
    const forged = { ...launch("forged.factory.testnet", 5), standard: "fair_chance" as const };
    expect(await loadCoinCards(factoryReader([forged]), 0, () => { throw new Error("no coin"); }, ctxFor("normal"))).toEqual([]);
  });

  it("reads the newest coins only", async () => {
    const events = Array.from({ length: MOST_COINS_SHOWN + 1 }, (_, i) => launch(`c${i}.factory.testnet`, i + 1));
    const seen: string[] = [];
    await loadCoinCards(factoryReader(events), 0, (c) => { seen.push(c); return readerFrom(recordedCoin("normal"), c); }, ctxFor("normal"));
    expect(seen).toHaveLength(MOST_COINS_SHOWN);
    expect(seen).not.toContain("c0.factory.testnet");
  });

  it("gives no cards if one coin cannot be read", async () => {
    const events = [launch("a.factory.testnet", 10), launch("b.factory.testnet", 20)];
    await expect(loadCoinCards(factoryReader(events), 0, (c) => readerFrom(recordedCoin(c.startsWith("a") ? "normal" : "rpc-error"), c), ctxFor("normal"))).rejects.toThrow();
  });
});
