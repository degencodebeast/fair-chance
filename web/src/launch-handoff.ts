// The launch result, handed from the launch form to the new coin's page in this browser tab, so
// the page shows the coin at once, before the chain indexers have it. The coin page reads the chain
// as usual; this only fills the header until those reads are drawn.
import { drawCoinTile } from "./coin-tile.ts";
import { el } from "./dom-elements.ts";

export interface LaunchedCoin {
  coin: string;
  name: string;
  ticker: string;
  icon: string | null;
  creator: string;
  launchBlock: number;
  /** Null when the page knows no explorer for the network: then no link shows. */
  explorerUrl: string | null;
}

const key = (coin: string) => `fc-launched:${coin}`;

export function saveLaunched(store: Pick<Storage, "setItem">, c: LaunchedCoin): void {
  try {
    store.setItem(key(c.coin), JSON.stringify(c));
  } catch {
    // A browser that keeps no storage: the coin page then waits for the chain reads.
  }
}

function isLaunched(v: unknown, coin: string): v is LaunchedCoin {
  const c = v as Partial<LaunchedCoin> | null;
  return (
    typeof c === "object" && c !== null && c.coin === coin &&
    typeof c.name === "string" && typeof c.ticker === "string" && typeof c.creator === "string" &&
    (c.icon === null || typeof c.icon === "string") && Number.isSafeInteger(c.launchBlock) &&
    (c.explorerUrl === null || (typeof c.explorerUrl === "string" && c.explorerUrl.startsWith("https://")))
  );
}

/** The saved launch of this coin; null when there is none or it is not the saved shape. */
export function takeLaunched(store: Pick<Storage, "getItem">, coin: string): LaunchedCoin | null {
  try {
    const text = store.getItem(key(coin));
    if (text === null) return null;
    const value: unknown = JSON.parse(text);
    return isLaunched(value, coin) ? value : null;
  } catch {
    return null;
  }
}

/** The header of a coin launched in this tab, from the launch result. */
export function drawJustLaunched(c: LaunchedCoin): HTMLElement {
  return el(
    "header",
    { class: "coin-header", id: "just-launched" },
    drawCoinTile({ account: c.coin, ticker: c.ticker, icon: c.icon }),
    el(
      "div",
      { class: "coin-title" },
      el("div", { class: "coin-name" }, el("h1", { id: "coin" }, c.name), el("span", { class: "num" }, c.ticker)),
      el(
        "div",
        { class: "chips" },
        el("span", { class: "chip" }, c.coin),
        el("span", { class: "chip" }, `Creator ${c.creator}`),
        el("span", { class: "chip" }, `Launched at block ${c.launchBlock}`),
      ),
      el("p", {}, "You launched this coin. The rest of the page loads from the chain."),
      c.explorerUrl !== null && el("a", { class: "text-link", href: c.explorerUrl, target: "_blank", rel: "noopener noreferrer" }, "See the launch on the testnet explorer"),
    ),
  );
}
