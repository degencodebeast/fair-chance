import { describe, expect, it } from "vitest";
import type { ChainReader, CoinEvent } from "../../src/chain.ts";
import { loadMyCoins } from "../../src/my-coins.ts";
import { HttpStatusError } from "../../src/refresher.ts";
import { sendAndRead } from "../../src/trade-actions.ts";
import { tradeResultText } from "../../src/trade-result-text.ts";
import { COMMA_REFUSAL } from "../../src/token-amount.ts";
import { buildLaunch } from "../../src/tx-builders.ts";

const launch = (coin: string, block: number): CoinEvent => ({ standard: "fair_chance_factory", version: "1", name: "fc_launch", coin: "factory.testnet", receiptId: `r${block}`, block, timeNs: null, data: { coin, creator: "x.testnet", launch_block: block } });
const meta = { spec: "ft-1.0.0", name: "A", symbol: "A", icon: null, decimals: 18 };

describe("adv", () => {
  it("F1: wallet error text with 'closed' after broadcast shown as nothing sent", async () => {
    const session = { accountId: "a.testnet", send: async () => { throw new Error("Connection closed while waiting for the transaction result"); } } as never;
    const out = await sendAndRead(session, []);
    expect(out.kind).toBe("unknown");
  });
  it("F2: events read has no backoff on 429", async () => {
    let n = 0;
    const factory = { events: async () => { n++; if (n === 1) throw new HttpStatusError("429", 429, null); return { events: [], cursor: 0 }; } } as unknown as ChainReader;
    const r = await loadMyCoins(factory, 0, () => ({}) as ChainReader, "x.testnet", { sleep: async () => {} }).catch((e) => `REJECTED: ${e.message}`);
    expect(typeof r).toBe("object");
  });
  it("F3: concurrent RPC requests in My coins", async () => {
    let cur = 0, max = 0;
    const reader = { view: async (m: string) => { cur++; max = Math.max(max, cur); await new Promise((r) => setTimeout(r, 20)); cur--; return m === "ft_metadata" ? meta : "0"; } } as unknown as ChainReader;
    const factory = { events: async () => ({ events: Array.from({ length: 12 }, (_, i) => launch(`c${i}.factory.testnet`, i)), cursor: 0 }) } as unknown as ChainReader;
    await loadMyCoins(factory, 0, () => reader, "x.testnet");
    expect(max).toBeLessThanOrEqual(4);
  });
  it("F4: result text rounding of small NEAR", () => {
    const t = tradeResultText({ kind: "result", result: { kind: "bought", nearIn: 99_999_999_999_999_999_999n, tokensOut: 5n, feeNear: 1n, markedTokens: 0n, explorerUrl: null } as never }, { symbol: "A", decimals: 18 });
    expect(t.headline).not.toContain("for 0.0000 NEAR");
  });
  it("F5: a comma in a launch opening buy is refused, never read as 15", () => {
    const l = buildLaunch("factory.testnet", { form: { name: "A", ticker: "AAA", description: "", openingBuy: "1,5" }, icon: null });
    expect(l).toEqual({ kind: "invalid", errors: [COMMA_REFUSAL] });
  });
});
