import { describe, expect, it } from "vitest";
import type { CoinEvent } from "../src/chain.ts";
import { changeText, linePoints, priceSeries } from "../src/price-series.ts";

const NEAR = 10n ** 24n;
const TOKEN = 10n ** 18n;

function event(name: string, block: number, data: Record<string, unknown>): CoinEvent {
  return { standard: "fair_chance", version: "1.0.0-rc.5", name, coin: "moth.factory.testnet", receiptId: `r${block}`, block, timeNs: `${block}000000000`, data };
}
const buy = (block: number, near: bigint, tokens: bigint) => event("fc_buy", block, { account: "a.testnet", near_in: String(near), tokens_out: String(tokens) });
const sell = (block: number, near: bigint, tokens: bigint) => event("fc_sell", block, { account: "a.testnet", near_out: String(near), tokens_sold: String(tokens) });

describe("price of each trade, from the swap events", () => {
  it("gives NEAR per 1M tokens for a buy and a sell, in block order", () => {
    const points = priceSeries([sell(30, NEAR / 2n, 4_000_000n * TOKEN), buy(10, NEAR, 5_000_000n * TOKEN)]);
    expect(points.map((p) => [p.block, p.nearPerMillion])).toEqual([
      [10, 0.2],
      [30, 0.125],
    ]);
    expect(points[0]!.timeNs).toBe("10000000000");
  });

  it("skips a sell that filled nothing and events that are not swaps", () => {
    const points = priceSeries([sell(5, 0n, 0n), event("fc_became_large", 6, { account: "x" }), buy(7, NEAR, 10_000_000n * TOKEN)]);
    expect(points).toHaveLength(1);
    expect(points[0]!.nearPerMillion).toBe(0.1);
  });

  it("gives no points for no events", () => {
    expect(priceSeries([])).toEqual([]);
  });
});

describe("price change text", () => {
  it("writes the change from the first trade to the last, cut to one decimal, with a sign", () => {
    const up = priceSeries([buy(1, NEAR, 10_000_000n * TOKEN), buy(2, NEAR, 8_000_000n * TOKEN)]);
    expect(changeText(up)).toBe("+25.0%");
    const down = priceSeries([buy(1, NEAR, 8_000_000n * TOKEN), buy(2, NEAR, 10_000_000n * TOKEN)]);
    expect(changeText(down)).toBe("-20.0%");
  });
  it("gives null with fewer than two trades", () => {
    expect(changeText([])).toBeNull();
    expect(changeText(priceSeries([buy(1, NEAR, TOKEN)]))).toBeNull();
  });
});

describe("line points for a chart", () => {
  it("puts the lowest price at the bottom and the highest at the top, inside a margin", () => {
    const points = priceSeries([buy(1, NEAR, 10_000_000n * TOKEN), buy(2, NEAR, 5_000_000n * TOKEN), buy(3, NEAR, 10_000_000n * TOKEN)]);
    const xy = linePoints(points, 200, 40, 3).split(" ").map((p) => p.split(",").map(Number) as [number, number]);
    expect(xy.map(([x]) => x)).toEqual([0, 100, 200]);
    expect(xy.map(([, y]) => y)).toEqual([37, 3, 37]);
  });
  it("draws a flat line in the middle when every price is equal", () => {
    const points = priceSeries([buy(1, NEAR, TOKEN), buy(2, NEAR, TOKEN)]);
    expect(linePoints(points, 200, 40, 3)).toBe("0,20 200,20");
  });
});
