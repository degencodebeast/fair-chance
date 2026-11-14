// The launch result, handed from the launch form to the new coin's page in this browser tab, so
// the page shows the coin at once, before the chain indexers have it. It is read once and removed.
// The page shows it only while its first chain read loads, only for the coin the link names, and
// only when that coin sits directly under the known factory with the ticker as its first part.
// The explorer link is built here from the transaction hash, never taken from storage.
import { drawCoinTile } from "./coin-tile.ts";
import { el } from "./dom-elements.ts";
import { LAUNCH_LIMITS } from "./launch-form-model.ts";
import { explorerTxLink } from "./tx-outcome.ts";

export interface LaunchedCoin {
  coin: string;
  name: string;
  ticker: string;
  icon: string | null;
  creator: string;
  launchBlock: number;
  /** The launch transaction; the explorer link comes from it. */
  txHash: string;
}

const key = (coin: string) => `fc-launched:${coin}`;

export function saveLaunched(store: Pick<Storage, "setItem">, c: LaunchedCoin): void {
  try {
    store.setItem(key(c.coin), JSON.stringify(c));
  } catch {
    // A browser that keeps no storage: the coin page then waits for the chain reads.
  }
}

function isLaunched(v: unknown, coin: string, factory: string): v is LaunchedCoin {
  const c = v as Partial<LaunchedCoin> | null;
  if (typeof c !== "object" || c === null || c.coin !== coin) return false;
  if (typeof c.name !== "string" || typeof c.ticker !== "string" || typeof c.creator !== "string" || typeof c.txHash !== "string") return false;
  if (c.name.length === 0 || c.name.length > LAUNCH_LIMITS.name || c.ticker.length > LAUNCH_LIMITS.ticker) return false;
  // The factory makes the coin account from the lowered ticker: <ticker>.<factory>, one level down.
  if (coin !== `${c.ticker.toLowerCase()}.${factory}`) return false;
  return (c.icon === null || typeof c.icon === "string") && Number.isSafeInteger(c.launchBlock);
}

/**
 * The saved launch of this coin under `factory`, removed from storage as it is read. Null when
 * there is none, when the link names no factory, or when it is not the saved shape.
 */
export function takeLaunched(store: Pick<Storage, "getItem" | "removeItem">, coin: string, factory: string | null): LaunchedCoin | null {
  try {
    const text = store.getItem(key(coin));
    if (text === null) return null;
    store.removeItem(key(coin));
    if (factory === null) return null;
    const value: unknown = JSON.parse(text);
    return isLaunched(value, coin, factory) ? value : null;
  } catch {
    return null;
  }
}

/** The header of a coin launched in this tab, from the launch result. */
export function drawJustLaunched(c: LaunchedCoin): HTMLElement {
  const link = explorerTxLink(c.txHash);
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
      link !== null && el("a", { class: "text-link", href: link, target: "_blank", rel: "noopener noreferrer" }, "See the launch on the testnet explorer"),
    ),
  );
}

/**
 * The note that stays on the new coin's page after its first chain read: the launch went through,
 * with the explorer link, as a buy and a sell show theirs.
 */
export function drawLaunchedNote(c: LaunchedCoin): HTMLElement {
  const link = explorerTxLink(c.txHash);
  return el(
    "div",
    { class: "trade-result trade-result-ok", id: "launched-note", role: "status" },
    el("p", {}, el("b", {}, `You launched ${c.name} (${c.ticker}).`)),
    link !== null && el("a", { class: "text-link", href: link, target: "_blank", rel: "noopener noreferrer" }, "See the launch on the testnet explorer"),
  );
}
