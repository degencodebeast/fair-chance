import { describe, expect, it } from "vitest";
import { chainReader, configFromUrl } from "../src/chain.ts";
import { receiptRows } from "../src/receipts.ts";
import { RECORDED, recordedFetch } from "./recorded-rpc.ts";

const coin = "walk.factory.sandbox";
const rpcUrl = "http://127.0.0.1:1";

describe("configFromUrl", () => {
  it("takes the coin account and the RPC URL from the page URL", () => {
    const cfg = configFromUrl(
      "http://localhost:5173/?coin=walk.factory.sandbox&rpc=http%3A%2F%2F127.0.0.1%3A53021",
    );
    expect(cfg).toEqual({ coin, rpcUrl: "http://127.0.0.1:53021" });
  });

  it("refuses a page URL without a coin or an RPC URL", () => {
    expect(() => configFromUrl("http://localhost/?rpc=http%3A%2F%2Fx")).toThrow(/coin/);
    expect(() => configFromUrl("http://localhost/?coin=a.near")).toThrow(/rpc/);
  });
});

describe("chain reader on recorded sandbox responses", () => {
  const reader = chainReader({ rpcUrl, coin }, recordedFetch(RECORDED, 27));

  it("reads a coin view", async () => {
    const rules = await reader.view<{ launch_block: number }>("fc_rules");
    expect(rules.launch_block).toBe(15);
  });

  it("finds the coin events by a block scan from the launch block", async () => {
    const { events, cursor } = await reader.events();
    expect(events.map((e) => e.name)).toEqual(["fc_buy", "fc_sell"]);
    expect(events[1]).toMatchObject({
      coin,
      block: 27,
      standard: "fair_chance",
      version: "1.0.0-rc.4",
      receiptId: "AieYQm9t4UEvo41tkdAFBiwHqnkDAfAi8zetodrZ7fRA",
    });
    expect(cursor).toBe(28);
  });

  it("maps the fc_sell event to one receipt row", async () => {
    const { events } = await reader.events();
    const rows = receiptRows(events);
    expect(rows).toEqual([
      {
        receipt_id: "AieYQm9t4UEvo41tkdAFBiwHqnkDAfAi8zetodrZ7fRA",
        block: 27,
        account: "alice.sandbox",
        tokens_asked: "4950495049504950495049504",
        tokens_sold: "4950495049504950495049504",
        near_out: "502487562189054726368159",
        fill: "full",
        limit_reason: null,
        text: "alice.sandbox sold 4.9M of 4.9M tokens for 0.50 NEAR. It filled in full.",
      },
    ]);
  });
});

describe("receipts page", () => {
  it("shows the recorded sell found by the block scan, from the page URL alone", async () => {
    const { loadReceiptsPage } = await import("../src/receipts-page.ts");
    const href = `http://localhost/?coin=${coin}&rpc=${encodeURIComponent(rpcUrl)}`;
    const page = await loadReceiptsPage(href, recordedFetch(RECORDED, 27));
    expect(page.source).toBe("block-scan");
    expect(page.launchBlock).toBe(15);
    expect(page.receipts.map((r) => r.receipt_id)).toEqual(["AieYQm9t4UEvo41tkdAFBiwHqnkDAfAi8zetodrZ7fRA"]);
  });
});

describe("block scan guard", () => {
  it("fails when the scan start is below the node's earliest block", async () => {
    const reader = chainReader({ rpcUrl, coin }, recordedFetch(RECORDED, 27, 20));
    await expect(reader.events(15)).rejects.toThrow(/no longer has the blocks.*15.*20/);
  });
});
