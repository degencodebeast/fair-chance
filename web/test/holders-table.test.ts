import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { ChainReader, CoinEvent } from "../src/chain.ts";
import { parseAccount } from "../src/account-views.ts";
import { holderRow, loadHolders, MOST_HOLDERS_READ } from "../src/holders-table.ts";
import { copyViolations } from "./copy-rules.ts";

const fixtures = join(import.meta.dirname, "../../testdata/account-plan-states");
const fixture = (name: string) => JSON.parse(readFileSync(join(fixtures, `${name}.json`), "utf8")) as { now_ns: string; total_supply: string; views: { fc_account: unknown } };
const ctx = (name: string) => ({ nowNs: BigInt(fixture(name).now_ns), timeZone: "UTC" });
const row = (name: string, who: string) => holderRow(who, parseAccount(fixture(name).views.fc_account), BigInt(fixture(name).total_supply), ctx(name));

describe("one holder row from the account view", () => {
  it("shows a large account held to the floor, in the limit colour, who cannot send", () => {
    expect(row("cap-full-floor", "you.testnet")).toEqual({
      account: "you.testnet", balance: "30.0M", share: "3.0%", tags: ["Large"], now: "100K now, more after 15:02", send: "No", limited: true,
    });
  });
  it("shows an early supply holder with the day of the next early window", () => {
    expect(row("early-supply-holder", "dart.testnet")).toMatchObject({
      balance: "5.0M", share: "0.5%", tags: ["Early supply"], now: "400K now, more after 13:02 tomorrow", send: "Yes", limited: true,
    });
  });
  it("shows a mid-size account that can sell all of it with no limit colour", () => {
    expect(row("band-below-one-allowance", "reed.testnet")).toMatchObject({ tags: ["Mid-size"], now: "All of it", send: "Yes", limited: false });
  });
  it("shows a small account", () => {
    expect(row("sell-fills-fully", "small.testnet")).toMatchObject({ tags: ["Small"], now: "All of it", limited: false });
  });
  it("follows the copy rules", () => {
    for (const [n, who] of [["cap-full-floor", "a"], ["early-supply-holder", "b"], ["band-below-one-allowance", "c"]] as const) {
      const r = row(n, who);
      for (const text of [r.balance, r.share, r.now, r.send, ...r.tags]) expect(copyViolations(text), text).toEqual([]);
    }
  });
});

function ev(name: string, block: number, data: Record<string, unknown>): CoinEvent {
  return { standard: "fair_chance", version: "1.0.0-rc.5", name, coin: "moth.factory.testnet", receiptId: `r${block}`, block, timeNs: null, data };
}
function readerOf(byAccount: Record<string, unknown>, asked: string[] = []): ChainReader {
  return {
    async view<T>(method: string, args?: object): Promise<T> {
      const id = (args as { account_id: string }).account_id;
      asked.push(`${method}:${id}`);
      if (!(id in byAccount)) throw new Error(`no account ${id}`);
      return byAccount[id] as T;
    },
    async account() { throw new Error("no"); },
    async accessKeyCount() { throw new Error("no"); },
    async events() { throw new Error("no"); },
  };
}
const big = fixture("cap-full-floor").views.fc_account as Record<string, unknown>;
const small = fixture("sell-fills-fully").views.fc_account as Record<string, unknown>;
const gone = { ...small, balance: "0", marked_balance: "0", unmarked_balance: "0" };
const supply = BigInt(fixture("cap-full-floor").total_supply);

describe("the holders table read from the accounts named in the events", () => {
  const events = [
    ev("fc_buy", 1, { account: "a.testnet" }),
    ev("fc_buy", 2, { account: "b.testnet" }),
    ev("fc_sell", 3, { account: "a.testnet" }),
    ev("fc_send", 4, { from: "a.testnet", to: "c.testnet" }),
    ev("fc_became_large", 5, { account: "d.testnet" }),
    ev("fc_cap_filled", 6, { by_account: "a.testnet" }),
  ];
  it("lists each account once, largest balance first, and leaves out an empty account", async () => {
    const asked: string[] = [];
    const t = await loadHolders(readerOf({ "a.testnet": small, "b.testnet": gone, "c.testnet": small, "d.testnet": big }, asked), events, supply, ctx("cap-full-floor"));
    expect(t.rows.map((r) => r.account)).toEqual(["d.testnet", "a.testnet", "c.testnet"]);
    expect(asked.sort()).toEqual(["fc_account:a.testnet", "fc_account:b.testnet", "fc_account:c.testnet", "fc_account:d.testnet"]);
    expect(t.notRead).toBe(0);
  });
  it("gives no table if one account cannot be read", async () => {
    await expect(loadHolders(readerOf({ "a.testnet": small }), events, supply, ctx("cap-full-floor"))).rejects.toThrow(/no account/);
  });
  it("reads the most recently active accounts only and says how many it left out", async () => {
    const many = Array.from({ length: MOST_HOLDERS_READ + 3 }, (_, i) => ev("fc_buy", i + 1, { account: `h${i}.testnet` }));
    const all = Object.fromEntries(many.map((e) => [e.data.account as string, small]));
    const t = await loadHolders(readerOf(all), many, supply, ctx("cap-full-floor"));
    expect(t.rows).toHaveLength(MOST_HOLDERS_READ);
    expect(t.notRead).toBe(3);
    expect(t.rows.map((r) => r.account)).not.toContain("h0.testnet");
  });
  it("gives an empty table for a coin with no events", async () => {
    expect(await loadHolders(readerOf({}), [], supply, ctx("cap-full-floor"))).toEqual({ rows: [], notRead: 0 });
  });
});
