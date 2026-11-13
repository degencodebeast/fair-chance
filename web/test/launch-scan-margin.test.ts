// The launch transaction sits in a block before launch_block (the factory receipt, then the coin
// batch), so every block scan of a coin starts LAUNCH_SCAN_MARGIN_BLOCKS before launch_block.
// Without the margin the page misses the opening buy's events.
import { describe, expect, it } from "vitest";
import { chainReader, LAUNCH_SCAN_MARGIN_BLOCKS, scanStartBlock, type ChainReader } from "../src/chain.ts";
import { loadCoinCard } from "../src/coin-cards-from-chain.ts";
import { loadCoinPage } from "../src/coin-page.ts";
import { loadReceiptsPage } from "../src/receipts-page.ts";
import { readerFrom, recordedCoin } from "./recorded-coin.ts";
import { RECORDED, recordedFetch } from "./recorded-rpc.ts";

const walkCoin = "walk.factory.sandbox";
const rpcUrl = "http://127.0.0.1:1";

/** The recorded fetch, and the block heights the scan asked for. */
function loggingFetch(): { fetchFn: typeof fetch; asked: number[] } {
  const base = recordedFetch(RECORDED, 27);
  const asked: number[] = [];
  const fetchFn: typeof fetch = async (input, init) => {
    const body = JSON.parse(String(init?.body ?? "{}")) as { method?: string; params?: { block_id?: unknown } };
    if (body.method === "block" && typeof body.params?.block_id === "number") asked.push(body.params.block_id);
    return base(input, init);
  };
  return { fetchFn, asked };
}

/** A reader that records the `since` of each events call. */
function recordingSince(reader: ChainReader, seen: (number | undefined)[]): ChainReader {
  return { ...reader, events: (since?: number) => (seen.push(since), reader.events(since)) };
}

describe("scan start", () => {
  it("is the launch block minus the margin, and never below block 0", () => {
    expect(LAUNCH_SCAN_MARGIN_BLOCKS).toBe(10);
    expect(scanStartBlock(15)).toBe(5);
    expect(scanStartBlock(10)).toBe(0);
    expect(scanStartBlock(3)).toBe(0);
  });

  it("the default scan asks for blocks from the margin before the launch block", async () => {
    const { fetchFn, asked } = loggingFetch();
    const { events } = await chainReader({ rpcUrl, coin: walkCoin }, fetchFn).events();
    expect(Math.min(...asked)).toBe(5);
    expect(events.map((e) => e.name)).toEqual(["fc_buy", "fc_sell"]);
  });

  it("the receipts page scans from the margin and lists each coin event once", async () => {
    const { fetchFn, asked } = loggingFetch();
    const href = `http://localhost/?coin=${walkCoin}&rpc=${encodeURIComponent(rpcUrl)}`;
    const page = await loadReceiptsPage(href, fetchFn);
    expect(Math.min(...asked)).toBe(5);
    expect(page.scanFrom).toBe(5);
    expect(page.events.map((e) => e.name)).toEqual(["fc_buy", "fc_sell"]);
  });

  it("the coin page and the coin card scan from the margin before the launch block", async () => {
    const coin = "moth.factory.testnet";
    const rec = recordedCoin("normal");
    const launchBlock = (rec.views.fc_rules as { launch_block: number }).launch_block;
    const seen: (number | undefined)[] = [];
    const href = `http://localhost/?coin=${coin}&rpc=${encodeURIComponent(rpcUrl)}`;
    await loadCoinPage(href, { reader: recordingSince(readerFrom(rec, coin), seen), nowNs: BigInt(rec.now_ns) });
    await loadCoinCard(recordingSince(readerFrom(rec, coin), seen), coin, { nowNs: BigInt(rec.now_ns), timeZone: "UTC" });
    expect(seen).toEqual([scanStartBlock(launchBlock), scanStartBlock(launchBlock)]);
  });
});
