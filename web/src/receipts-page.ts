// What the receipts page shows, from the page URL alone. The browser page and the
// command-line read both call this, so they show the same receipts.
import { chainReader, configFromUrl, scanStartBlock } from "./chain.ts";
import { receiptRows, type ReceiptRow } from "./receipts.ts";

export interface ReceiptsPage {
  coin: string;
  rpcUrl: string;
  /** Where the receipts come from: the block scan from just before the coin's launch block. */
  source: "block-scan";
  launchBlock: number;
  /** The first block the scan read: scanStartBlock(launchBlock). */
  scanFrom: number;
  /** Every coin event the scan found, in chain order. */
  events: { name: string; receiptId: string; block: number }[];
  receipts: ReceiptRow[];
}

export async function loadReceiptsPage(href: string, fetchFn: typeof fetch = fetch): Promise<ReceiptsPage> {
  const cfg = configFromUrl(href);
  const reader = chainReader(cfg, fetchFn);
  const rules = await reader.view<{ launch_block: number }>("fc_rules");
  const scanFrom = scanStartBlock(rules.launch_block);
  const { events } = await reader.events(scanFrom);
  return {
    coin: cfg.coin,
    rpcUrl: cfg.rpcUrl,
    source: "block-scan",
    launchBlock: rules.launch_block,
    scanFrom,
    events: events.map((e) => ({ name: e.name, receiptId: e.receiptId, block: e.block })),
    receipts: receiptRows(events),
  };
}
