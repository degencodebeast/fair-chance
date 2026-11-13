import { describe, expect, it } from "vitest";
import { coinListModel, type CoinCard } from "../src/coin-list-model.ts";
import { SAMPLE_COINS } from "../src/coin-list-sample.ts";
import { allText, copyViolations } from "./copy-rules.ts";

function card(over: Partial<CoinCard>): CoinCard {
  return {
    account: "a.factory.testnet", name: "Alpha", ticker: "ALP", age: "2 h", ageSeconds: 7200, marketCapNear: 100, marketCap: "100.00 NEAR",
    change: "+1.0%", early: "10.0%", cap: { label: "Cap open", tone: "open" }, latest: { account: "", text: "No sells yet." },
    bigSellHeld: false, inEarlyWindow: false, lastSellAtNs: null, line: "", source: "chain", ...over,
  };
}
const alpha = card({});
const bravo = card({ account: "b.factory.testnet", name: "Bravo", ticker: "BRV", marketCapNear: 300, marketCap: "300.00 NEAR", ageSeconds: 600, age: "10 min", lastSellAtNs: 5n, bigSellHeld: true });
const cold = card({ account: "c.factory.testnet", name: "Cold Fish", ticker: "FSH", marketCapNear: 50, marketCap: "50.00 NEAR", ageSeconds: 5, age: "5 s", inEarlyWindow: true });
const cards = [alpha, bravo, cold];

describe("the coin list", () => {
  it("lists by market cap, largest first, by default", () => {
    expect(coinListModel(cards, { tab: "Market cap", query: "" }).visible.map((c) => c.name)).toEqual(["Bravo", "Alpha", "Cold Fish"]);
  });
  it("filters each tab by its own rule", () => {
    const names = (tab: Parameters<typeof coinListModel>[1]["tab"]) => coinListModel(cards, { tab, query: "" }).visible.map((c) => c.name);
    expect(names("New")).toEqual(["Cold Fish", "Bravo"]); // under 1 h old, newest first
    expect(names("Active")).toEqual(["Bravo"]); // has a sell
    expect(names("Early window now")).toEqual(["Cold Fish"]);
    expect(names("Big sells held")).toEqual(["Bravo"]);
  });
  it("searches name, ticker and coin account, and counts follow the search", () => {
    const m = coinListModel(cards, { tab: "Market cap", query: "  fsh " });
    expect(m.visible.map((c) => c.name)).toEqual(["Cold Fish"]);
    expect(coinListModel(cards, { tab: "Market cap", query: "b.factory" }).visible.map((c) => c.name)).toEqual(["Bravo"]);
    expect(coinListModel(cards, { tab: "Market cap", query: "alpha" }).visible).toHaveLength(1);
    expect(m.tabs.map((t) => [t.label, t.count])).toEqual([["Market cap", 1], ["New", 1], ["Active", 0], ["Early window now", 1], ["Big sells held", 0]]);
  });
  it("shows the top four by market cap", () => {
    const many = ["1", "2", "3", "4", "5"].map((n, i) => card({ account: `${n}.f.testnet`, name: `C${n}`, marketCapNear: i * 10 }));
    expect(coinListModel(many, { tab: "Market cap", query: "" }).top.map((c) => c.name)).toEqual(["C5", "C4", "C3", "C2"]);
  });
  it("says what is empty", () => {
    expect(coinListModel(cards, { tab: "Market cap", query: "xyz" }).emptyText).toBe('No coin matches "xyz".');
    expect(coinListModel([], { tab: "Market cap", query: "" }).emptyText).toMatch(/^No coins yet\./);
    expect(coinListModel([alpha], { tab: "Big sells held", query: "" }).emptyText).toBe("No big sell was limited in the last hour.");
    expect(coinListModel([alpha], { tab: "Market cap", query: "" }).emptyText).toBeNull();
  });
  it("puts a coin with no market cap last in the market cap tab", () => {
    expect(coinListModel([card({ name: "Blank", marketCapNear: null, marketCap: "Not available" }), alpha], { tab: "Market cap", query: "" }).visible[0]!.name).toBe("Alpha");
  });
});

describe("sample coins", () => {
  it("are all marked as sample, none points at a chain account the page could open", () => {
    expect(SAMPLE_COINS.length).toBeGreaterThanOrEqual(5);
    for (const c of SAMPLE_COINS) {
      expect(c.source).toBe("sample");
      expect(c.account.endsWith(".sample")).toBe(true);
    }
  });
  it("follow the copy rules", () => {
    for (const text of allText(SAMPLE_COINS)) expect(copyViolations(text), text).toEqual([]);
  });
  it("fill every tab, so each tab has a card to look at", () => {
    const m = (tab: Parameters<typeof coinListModel>[1]["tab"]) => coinListModel(SAMPLE_COINS, { tab, query: "" }).visible.length;
    for (const tab of ["Market cap", "New", "Active", "Early window now", "Big sells held"] as const) expect(m(tab)).toBeGreaterThan(0);
  });
});
