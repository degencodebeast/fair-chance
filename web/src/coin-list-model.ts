// The coin list as data: cards, tabs, search, top coins and the empty texts. Pure; the renderer
// only draws what this returns.
export type CapTone = "full" | "early" | "open";
export type ListTab = "Market cap" | "New" | "Active" | "Early window now" | "Big sells held";
export const LIST_TABS: ListTab[] = ["Market cap", "New", "Active", "Early window now", "Big sells held"];

export interface CoinCard {
  /** The coin account. A sample coin has a name ending in ".sample", which is no chain account. */
  account: string;
  name: string;
  ticker: string;
  age: string;
  ageSeconds: number;
  /** For sorting; null when the coin has no pool reading. */
  marketCapNear: number | null;
  marketCap: string;
  /** Change from the earliest trade to the latest; null with fewer than two trades. */
  change: string | null;
  early: string;
  cap: { label: string; tone: CapTone };
  latest: { account: string; text: string };
  /** A big sell was limited in the last hour. */
  bigSellHeld: boolean;
  inEarlyWindow: boolean;
  /** Time of the latest sell, for ordering the Active tab; null when there is none. */
  lastSellAtNs: bigint | null;
  /** SVG polyline points of the price line, "" when there is none. */
  line: string;
  /** "chain": read from the chain. "sample": made-up, and the card says so. */
  source: "chain" | "sample";
}

const NEW_UNDER_SECONDS = 3600;

const EMPTY_TEXT: Record<ListTab, string> = {
  "Market cap": "No coins yet. Each new coin shows here with its cap state and latest receipt.",
  "Early window now": "No coin is in its early window right now. A new coin shows here during its early window, blocks 0 to 17, about 10 seconds.",
  New: "No coin is under 1 h old. Each new coin shows here with its age, market cap, early-supply share, cap state and latest receipt.",
  Active: "No sells yet on any coin. Coins with recent sells show here, each with its latest receipt.",
  "Big sells held": "No big sell was limited in the last hour.",
};

function inTab(c: CoinCard, tab: ListTab): boolean {
  switch (tab) {
    case "Market cap":
      return true;
    case "New":
      return c.ageSeconds < NEW_UNDER_SECONDS;
    case "Active":
      return c.lastSellAtNs !== null;
    case "Early window now":
      return c.inEarlyWindow;
    case "Big sells held":
      return c.bigSellHeld;
  }
}

function matches(c: CoinCard, query: string): boolean {
  return query === "" || c.name.toLowerCase().includes(query) || c.ticker.toLowerCase().includes(query) || c.account.toLowerCase().includes(query);
}

function byMarketCap(a: CoinCard, b: CoinCard): number {
  return (b.marketCapNear ?? -1) - (a.marketCapNear ?? -1);
}

function sorted(cards: CoinCard[], tab: ListTab): CoinCard[] {
  const out = [...cards];
  if (tab === "Active") return out.sort((a, b) => (a.lastSellAtNs! < b.lastSellAtNs! ? 1 : a.lastSellAtNs! > b.lastSellAtNs! ? -1 : 0));
  if (tab === "Market cap" || tab === "Big sells held") return out.sort(byMarketCap);
  return out.sort((a, b) => a.ageSeconds - b.ageSeconds);
}

export interface CoinListModel {
  tabs: { label: ListTab; count: number }[];
  visible: CoinCard[];
  top: CoinCard[];
  emptyText: string | null;
  tabNote: string | null;
}

export function coinListModel(cards: CoinCard[], pick: { tab: ListTab; query: string }): CoinListModel {
  const query = pick.query.trim().toLowerCase();
  const searched = cards.filter((c) => matches(c, query));
  const visible = sorted(searched.filter((c) => inTab(c, pick.tab)), pick.tab);
  let emptyText: string | null = null;
  if (visible.length === 0) emptyText = query !== "" ? `No coin matches "${pick.query.trim()}".` : EMPTY_TEXT[pick.tab];
  return {
    tabs: LIST_TABS.map((label) => ({ label, count: searched.filter((c) => inTab(c, label)).length })),
    visible,
    top: [...cards].sort(byMarketCap).slice(0, 4),
    emptyText,
    tabNote: pick.tab === "Big sells held" && visible.length > 0 ? "Coins where a big sell was limited in the last hour, with the latest receipt." : null,
  };
}
