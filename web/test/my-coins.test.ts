// My coins on a fake factory and fake coin reads.
import { describe, expect, it } from "vitest";
import type { ChainReader, CoinEvent } from "../src/chain.ts";
import { loadMyCoins } from "../src/my-coins.ts";
import { HttpStatusError } from "../src/refresher.ts";

const ME = "you.testnet";
const launch = (coin: string, creator: string, block: number): CoinEvent => ({
  standard: "fair_chance_factory", version: "1", name: "fc_launch", coin: "factory.testnet", receiptId: `r${block}`, block, timeNs: null, data: { coin, creator, launch_block: block },
});

function coinReader(views: Record<string, unknown>, calls: string[]): ChainReader {
  return {
    async view<T>(method: string, args?: object) {
      calls.push(`${method} ${JSON.stringify(args ?? {})}`);
      if (!(method in views)) throw new Error(`no view ${method}`);
      return views[method] as T;
    },
    account: async () => { throw new Error("unused"); },
    accessKeyCount: async () => 0,
    events: async () => ({ events: [], cursor: 0 }),
  };
}

const meta = (name: string, symbol: string) => ({ spec: "ft-1.0.0", name, symbol, icon: null, decimals: 18 });

describe("my coins", () => {
  it("lists the coins the account launched and the coins it holds, each with what it can sell now", async () => {
    const calls: string[] = [];
    const factory: ChainReader = { ...coinReader({}, calls), events: async () => ({ events: [launch("a.factory.testnet", ME, 10), launch("b.factory.testnet", "other.testnet", 20), launch("c.factory.testnet", "other.testnet", 30)], cursor: 0 }) };
    const coins: Record<string, Record<string, unknown>> = {
      "a.factory.testnet": { ft_metadata: meta("Apple", "APL"), ft_balance_of: "0" },
      "b.factory.testnet": { ft_metadata: meta("Bee", "BEE"), ft_balance_of: "5000", fc_quote_sell: { tokens_filled: "2000", near_out: "1", penalty_near: "0", fill: "partial", limit_reason: "allowance" } },
      "c.factory.testnet": { ft_metadata: meta("Cat", "CAT"), ft_balance_of: "0" },
    };
    const mine = await loadMyCoins(factory, 0, (coin) => coinReader(coins[coin]!, calls), ME);
    expect(mine.launched.map((c) => c.coin)).toEqual(["a.factory.testnet"]);
    expect(mine.launched[0]).toMatchObject({ name: "Apple", ticker: "APL", balance: 0n, canSellNow: 0n, launchBlock: 10 });
    expect(mine.held.map((c) => c.coin)).toEqual(["b.factory.testnet"]);
    expect(mine.held[0]).toMatchObject({ name: "Bee", ticker: "BEE", balance: 5000n, canSellNow: 2000n, decimals: 18 });
    expect(calls).toContain(`ft_balance_of {"account_id":"${ME}"}`);
    expect(calls).toContain(`fc_quote_sell {"account_id":"${ME}","amount":"5000"}`);
    expect(calls.filter((c) => c.startsWith("fc_quote_sell"))).toHaveLength(1);
  });

  it("fails as a whole when one read fails, so no list mixes reads", async () => {
    const factory: ChainReader = { ...coinReader({}, []), events: async () => ({ events: [launch("a.factory.testnet", ME, 10)], cursor: 0 }) };
    await expect(loadMyCoins(factory, 0, () => coinReader({ ft_metadata: meta("A", "A") }, []), ME)).rejects.toThrow("no view ft_balance_of");
  });
});

describe("my coins reads", () => {
  const many = (n: number) => Array.from({ length: n }, (_, i) => launch(`c${i}.factory.testnet`, "other.testnet", i + 1));

  it("reads at most 4 coins at a time", async () => {
    let inFlight = 0;
    let most = 0;
    const factory: ChainReader = { ...coinReader({}, []), events: async () => ({ events: many(12), cursor: 0 }) };
    const slow = (): ChainReader => ({
      ...coinReader({}, []),
      async view<T>(method: string) {
        inFlight++;
        most = Math.max(most, inFlight);
        await new Promise((r) => setTimeout(r, 1));
        inFlight--;
        return (method === "ft_metadata" ? meta("X", "X") : "0") as T;
      },
    });
    const mine = await loadMyCoins(factory, 0, slow, ME);
    expect(mine.held).toEqual([]);
    // One view at a time per coin, 4 coins at a time: at most 4 RPC calls in flight.
    expect(most).toBe(4);
  });

  it("waits and reads a coin again after a 429, with the shared backoff", async () => {
    const waits: number[] = [];
    let tries = 0;
    const factory: ChainReader = { ...coinReader({}, []), events: async () => ({ events: many(1), cursor: 0 }) };
    const flaky = (): ChainReader => ({
      ...coinReader({}, []),
      async view<T>(method: string) {
        if (method === "ft_balance_of" && tries++ === 0) throw new HttpStatusError("busy", 429, 9_000);
        return (method === "ft_metadata" ? meta("X", "X") : "0") as T;
      },
    });
    const mine = await loadMyCoins(factory, 0, flaky, ME, { sleep: async (ms) => void waits.push(ms) });
    expect(mine.launched).toEqual([]);
    expect(waits).toEqual([9_000]);
  });

  it("reads the factory events again after a 429, with the shared backoff", async () => {
    const waits: number[] = [];
    let reads = 0;
    const factory: ChainReader = {
      ...coinReader({}, []),
      events: async () => {
        if (reads++ === 0) throw new HttpStatusError("busy", 429, null);
        return { events: [], cursor: 0 };
      },
    };
    expect(await loadMyCoins(factory, 0, () => coinReader({}, []), ME, { sleep: async (ms) => void waits.push(ms) })).toEqual({ launched: [], held: [] });
    expect(waits).toEqual([4_000]);
  });

  it("does not retry a read that failed for another reason", async () => {
    const waits: number[] = [];
    const factory: ChainReader = { ...coinReader({}, []), events: async () => ({ events: many(1), cursor: 0 }) };
    const broken = (): ChainReader => ({ ...coinReader({}, []), view: async () => { throw new HttpStatusError("gone", 404, null); } });
    await expect(loadMyCoins(factory, 0, broken, ME, { sleep: async (ms) => void waits.push(ms) })).rejects.toThrow("gone");
    expect(waits).toEqual([]);
  });
});
