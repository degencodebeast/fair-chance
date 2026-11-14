// The plain words of a trade result, from real testnet outcomes.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { tradeResultText } from "../src/trade-result-text.ts";
import { parseTxOutcome } from "../src/tx-outcome.ts";
import type { FinalOutcome } from "../src/tx-types.ts";
import { copyViolations } from "./copy-rules.ts";

const result = (name: string) => parseTxOutcome(JSON.parse(readFileSync(join(import.meta.dirname, "fixtures/tx-outcomes", `${name}.json`), "utf8")) as FinalOutcome);
const coin = { symbol: "CABAL2", decimals: 18 };

describe("the words of a trade result", () => {
  it("says what a buy got and links the explorer", () => {
    const t = tradeResultText({ kind: "result", result: result("bag-buy") }, { symbol: "BAG", decimals: 18 });
    expect(t.tone).toBe("ok");
    expect(t.headline).toBe("You bought 7.3M BAG for 1.0000 NEAR.");
    expect(t.lines).toContain("Swap fee: 0.0100 NEAR.");
    expect(t.explorerUrl).toBe(result("bag-buy").explorerUrl);
  });

  it("says a full sell sold all of it", () => {
    const t = tradeResultText({ kind: "result", result: result("cabal2-sell-full") }, coin);
    expect(t.headline).toBe("You sold 2.7M CABAL2 for 0.2922 NEAR.");
    expect(t.lines).toEqual([]);
  });

  it("says what a partial sell sold, what stays and why", () => {
    const t = tradeResultText({ kind: "result", result: result("cabal2-sell-partial") }, coin);
    expect(t.tone).toBe("limit");
    expect(t.headline).toBe("You sold 96.4K of 3.9M CABAL2 for 0.0104 NEAR.");
    expect(t.lines).toContain("3.8M CABAL2 stay in your wallet.");
    expect(t.lines).toContain("The pool cap is full for this hour, so just a small floor fills.");
  });

  it("gives the refusal sentence for a refused sell", () => {
    const t = tradeResultText({ kind: "result", result: result("cabal2-sell-early-window") }, coin);
    expect(t.tone).toBe("refused");
    expect(t.headline).toBe("The coin refused this transaction, so nothing changed.");
    expect(t.lines).toEqual(["Early supply cannot sell in the early blocks, and selling opens when the early window ends."]);
    expect(t.explorerUrl).toMatch(/^https:\/\/testnet\.nearblocks\.io\/txns\//);
  });

  it("gives one sentence and no link when nothing was sent", () => {
    const t = tradeResultText({ kind: "not-sent", text: "The wallet closed and sent nothing." }, coin);
    expect(t).toEqual({ tone: "refused", headline: "The wallet closed and sent nothing.", lines: [], explorerUrl: null });
  });

  it("follows the copy rules", () => {
    for (const name of ["bag-buy", "cabal2-sell-full", "cabal2-sell-partial", "cabal2-sell-early-window"]) {
      const t = tradeResultText({ kind: "result", result: result(name) }, coin);
      for (const line of [t.headline, ...t.lines]) expect(copyViolations(line), line).toEqual([]);
    }
  });
});
