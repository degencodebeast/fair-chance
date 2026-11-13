import { describe, expect, it, vi } from "vitest";
import { loadCoinPage } from "../src/coin-page.ts";
import { coinPageModel, loadingFrame, type CoinPageModel } from "../src/coin-page-model.ts";
import { nearShort, sellRow, tokensShort } from "../src/receipts.ts";
import { telegramButton } from "../src/telegram-button.ts";
import { allText, copyViolations } from "./copy-rules.ts";
import { readerFrom, recordedCoin, type RecordedCoin } from "./recorded-coin.ts";

const coin = "moth.factory.testnet";
const href = `http://localhost/?coin=${coin}&rpc=${encodeURIComponent("http://127.0.0.1:1")}&factory=factory.testnet&code_hash=DmuJEGxMYTS5hjKvhyYpcJmoUBwHXmBRjCpWWDhbKXSa`;
const E18 = "000000000000000000";

function line(event: string, data: Record<string, unknown>): string {
  return `EVENT_JSON:${JSON.stringify({ standard: "fair_chance", version: "1.0.0-rc.7", event, data: [data] })}`;
}

async function view(rec: RecordedCoin, link = href) {
  const nowNs = BigInt(rec.now_ns);
  const data = await loadCoinPage(link, { reader: readerFrom(rec, coin), nowNs });
  return coinPageModel(data, { nowNs, timeZone: "UTC" });
}
async function ready(rec: RecordedCoin): Promise<CoinPageModel> {
  const v = await view(rec);
  if (v.state !== "ready") throw new Error("not ready");
  return v;
}

describe("receipts for buys and sends", () => {
  it("shows an early-supply buy and a creator buy with compact amounts", async () => {
    const rec = recordedCoin("early-window-live");
    rec.events.push({
      receipt_id: "BuyDart", block: 104, time_ns: "1794579836000000000",
      line: line("fc_buy", { account: "dart.testnet", near_in: "1", fee_near: "0", creator_fee_near: "0", tokens_out: `9200000${E18}`, marked_tokens: `9200000${E18}`, class_after: "small", block: 104 }),
    });
    const v = await ready(rec);
    const bought = v.receipts.map((r) => [r.kind, r.sentence]);
    expect(bought).toContainEqual(["Early supply", "dart.testnet bought 9.2M tokens in block 4. These tokens count as early supply."]);
    expect(bought).toContainEqual(["Creator group", "lamp.testnet (creator group) bought 10.0M tokens in block 1. These tokens count as early supply."]);
  });

  it("does not list a buy that marks no tokens", async () => {
    const rec = recordedCoin("early-window-live");
    rec.events = [{
      receipt_id: "Plain", block: 150, time_ns: "1794579836000000000",
      line: line("fc_buy", { account: "x.testnet", near_in: "1", fee_near: "0", creator_fee_near: "0", tokens_out: `5${E18}`, marked_tokens: "0", class_after: "small", block: 150 }),
    }];
    expect((await ready(rec)).receipts).toEqual([]);
  });

  it("shows a send, and says when the receiver's tokens count as early supply", async () => {
    const rec = recordedCoin("normal");
    rec.events.push({
      receipt_id: "SendA", block: 2300, time_ns: "1794581000000000000",
      line: line("fc_send", { from: "a.testnet", to: "b.testnet", amount: `1500000${E18}`, marked_amount: "0", to_marked_amount: `1500000${E18}` }),
    }, {
      receipt_id: "SendB", block: 2301, time_ns: "1794581001000000000",
      line: line("fc_send", { from: "a.testnet", to: "c.testnet", amount: `200${E18}`, marked_amount: "0", to_marked_amount: "0" }),
    });
    const v = await ready(rec);
    const sends = v.receipts.filter((r) => r.kind === "Send").map((r) => r.sentence);
    expect(sends).toEqual([
      "a.testnet sent 200.0 tokens to c.testnet.",
      "a.testnet sent 1.5M tokens to b.testnet. All of them count as early supply for b.testnet.",
    ]);
    expect(v.receipts.every((r) => r.sellReceiptId === null || r.kind !== "Send")).toBe(true);
  });
});

describe("receipt text", () => {
  const sell = (fill: string, sold: string) => ({
    standard: "fair_chance", name: "fc_sell", coin, block: 1, receiptId: "r", timeNs: "1",
    data: { account: "you.testnet", tokens_asked: `30000000${E18}`, tokens_sold: sold, near_out: "20000000000000000000000", fill, limit_reason: null },
  }) as never;
  it("uses compact amounts and plain words for the fill", () => {
    expect(sellRow(sell("partial", `100000${E18}`)).text).toBe("you.testnet sold 100K of 30.0M tokens for 0.02 NEAR. It filled in part.");
    expect(sellRow(sell("full", `30000000${E18}`)).text).toBe("you.testnet sold 30.0M of 30.0M tokens for 0.02 NEAR. It filled in full.");
    expect(sellRow(sell("none", "0")).text).toBe("you.testnet sold 0 of 30.0M tokens for 0.02 NEAR. Nothing filled.");
  });
});

describe("summary lines", () => {
  it("shows no label twice and uses the v5 strings", async () => {
    const v = await ready(recordedCoin("normal"));
    expect(v.summaryLines).toEqual([
      { label: "Early supply", value: "18.4% of supply", tone: "plain" },
      { label: "Large holders", value: "2 now", tone: "plain" },
      { label: "Pool cap", value: "1.23 of 2.13 NEAR room left, resets 15:02", tone: "plain" },
    ]);
  });
  it("says full, resets HH:MM and the locked early supply", async () => {
    const full = await ready(recordedCoin("cap-full"));
    expect(full.summaryLines[2]).toEqual({ label: "Pool cap", value: "full, resets 15:02", tone: "limit" });
    const early = await ready(recordedCoin("early-window-live"));
    expect(early.summaryLines[0]!.value).toBe("no sells until block 17");
  });
});

describe("receipt count", () => {
  it("uses the singular for one sell receipt", async () => {
    const v = await ready(recordedCoin("normal"));
    expect(v.status).toBe("1 sell receipt from block 100, read from the chain.");
    const none = await ready(recordedCoin("empty-receipts"));
    expect(none.status).toMatch(/^0 sell receipts from block/);
  });
});

describe("loading and error frames", () => {
  it("keeps the frame with Loading in each card", () => {
    const f = loadingFrame("Loading");
    expect(f.stats.every((s) => s.value === "Loading")).toBe(true);
    expect(f.who.map((c) => c.text)).toEqual(["Loading", "Loading", "Loading"]);
    expect(f.summaryLines.map((l) => l.value)).toEqual(["Loading", "Loading", "Loading"]);
    expect(f.receipts).toBe("Loading");
  });
  it("names the network in the error and keeps the frame with Not loaded", async () => {
    const v = await view(recordedCoin("rpc-error"));
    if (v.state !== "error") throw new Error("expected error");
    expect(v.title).toBe("Values cannot load from the local sandbox.");
    expect(v.frame.who.map((c) => c.text)).toEqual(["Not loaded", "Not loaded", "Not loaded"]);
    expect(v.frame.receipts).toBe("Not loaded");
    const testnet = await view(recordedCoin("rpc-error"), `http://localhost/?coin=${coin}&network=testnet`);
    expect(testnet.state === "error" && testnet.title).toBe("Values cannot load from NEAR testnet.");
    expect(allText(v).flatMap(copyViolations)).toEqual([]);
  });
});

describe("header", () => {
  it("has the rules chip text for desktop and phone, and a turned-off Telegram button with a reason", async () => {
    const v = await ready(recordedCoin("normal"));
    expect(v.header.rulesChip).toEqual({ desktop: "Same rules for every coin", phone: "Rules" });
    expect(v.telegram).toEqual({ label: "Watch on Telegram", reason: "Not open yet. The public alert bot is not live." });
  });
});

describe("compact amounts that hide a difference", () => {
  const sell = (asked: string, sold: string) => ({
    standard: "fair_chance", name: "fc_sell", coin, block: 1, receiptId: "r", timeNs: "1",
    data: { account: "you.testnet", tokens_asked: asked, tokens_sold: sold, near_out: "20000000000000000000000", fill: "partial", limit_reason: null },
  }) as never;
  it("shows more digits when sold and asked share a compact form", () => {
    expect(sellRow(sell(`4999999${E18}`, `4950000${E18}`)).text).toBe(
      "you.testnet sold 4,950,000.00 of 4,999,999.00 tokens for 0.02 NEAR. It filled in part.",
    );
  });
  it("keeps the compact form when the amounts are equal", () => {
    expect(sellRow(sell(`4999999${E18}`, `4999999${E18}`)).text).toContain("sold 4.9M of 4.9M tokens");
  });
});

describe("tiny amounts", () => {
  it("says less than 0.1 tokens and less than 0.01 NEAR instead of a zero", () => {
    expect(tokensShort("1")).toBe("less than 0.1");
    expect(tokensShort("0")).toBe("0");
    expect(nearShort("1")).toBe("less than 0.01");
    expect(nearShort("0")).toBe("0.00");
  });
});

describe("Telegram button element", () => {
  it("is disabled and shows its reason", () => {
    const made = (tag: string) => {
      const attrs = new Map<string, string>();
      const kids: unknown[] = [];
      return { tag, attrs, kids, setAttribute: (k: string, v: string) => attrs.set(k, v), append: (c: unknown) => kids.push(c) };
    };
    vi.stubGlobal("document", { createElement: made });
    try {
      const wrap = telegramButton({ label: "Watch on Telegram", reason: "Not open yet." }) as never as ReturnType<typeof made>;
      const [button, reason] = wrap.kids as ReturnType<typeof made>[];
      expect(button!.attrs.get("id")).toBe("watch-telegram");
      expect(button!.attrs.has("disabled")).toBe(true);
      expect(button!.kids).toEqual(["Watch on Telegram"]);
      expect(reason!.attrs.get("id")).toBe("watch-telegram-reason");
      expect(reason!.kids).toEqual(["Not open yet."]);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
