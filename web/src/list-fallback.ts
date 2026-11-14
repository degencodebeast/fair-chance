// What the coin list shows when the link names no chain to read. Made-up coins stay on the visitor's
// own machine; a hosted page shows chain data or nothing.
import { SAMPLE_COINS } from "./coin-list-sample.ts";
import type { CoinCard } from "./coin-list-model.ts";
import { isOwnMachineHost } from "./local-host.ts";
import type { ListLink } from "./page-link.ts";

export function fallbackList(link: Exclude<ListLink, { kind: "chain" }>, hostname: string): { cards: CoinCard[]; note: string } {
  if (isOwnMachineHost(hostname)) {
    return {
      cards: SAMPLE_COINS,
      note: link.kind === "sample" ? "Sample data. This link names no chain, so every coin below is made up." : `${link.source} has no factory set yet, so every coin below is sample data.`,
    };
  }
  return { cards: [], note: link.kind === "sample" ? "This link names no chain, so there are no coins to show." : `${link.source} has no factory set yet, so there are no coins to show.` };
}
