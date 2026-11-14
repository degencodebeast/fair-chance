// Draws the coin list. With no factory to read, the cards are sample coins and each says so. With a
// factory, the cards come from its launch events. The list model decides what shows; this file draws.
import { chainReader } from "./chain.ts";
import { loadCoinCards } from "./coin-cards-from-chain.ts";
import { coinListModel, type CoinCard, type ListTab } from "./coin-list-model.ts";
import { drawCoinTile } from "./coin-tile.ts";
import { el, type Child } from "./dom-elements.ts";
import { resolveListLink, type ListLink } from "./page-link.ts";
import { fallbackList } from "./list-fallback.ts";
import { sampleLabel } from "./sample-label.ts";
import { screenHeader } from "./screen-chrome.ts";
import { screenLink } from "./screen-route.ts";
import { refreshWhileShown } from "./refresher.ts";

const SVG = "http://www.w3.org/2000/svg";

function sparkline(c: CoinCard): Node {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", "0 0 200 40");
  svg.setAttribute("preserveAspectRatio", "none");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", c.change ? `Price of each trade, ${c.change}` : "Price of each trade");
  svg.setAttribute("class", "spark");
  if (c.line.includes(" ")) {
    const line = document.createElementNS(SVG, "polyline");
    line.setAttribute("points", c.line);
    line.setAttribute("fill", "none");
    line.setAttribute("stroke", "#17150F");
    line.setAttribute("stroke-width", "2");
    line.setAttribute("vector-effect", "non-scaling-stroke");
    svg.append(line);
  }
  return svg;
}

function cardBody(c: CoinCard): Child[] {
  return [
    el("div", { class: "coin-card-head" }, drawCoinTile({ account: c.account, ticker: c.ticker, icon: c.icon, small: true }), el("div", { class: "coin-card-title" }, el("div", {}, el("span", { class: "coin-card-name" }, c.name), " ", el("span", { class: "num" }, c.ticker)), el("div", { class: "num muted" }, c.age))),
    el("div", { class: "spark-row" }, sparkline(c), el("span", { class: "num" }, c.change ? `Trades ${c.change}` : "Fewer than 2 trades")),
    el("span", { class: `chip cap-chip cap-${c.cap.tone}` }, c.cap.label),
    el("div", { class: "coin-card-facts" }, el("div", {}, el("div", { class: "muted" }, "Market cap"), el("div", { class: "num" }, c.marketCap)), el("div", {}, el("div", { class: "muted" }, "Early supply"), el("div", { class: "num" }, c.early))),
    el("div", { class: "latest" }, el("b", {}, "Latest receipt. "), c.latest.account && el("span", { class: "num" }, `${c.latest.account} `), c.latest.text),
    c.source === "sample" && sampleLabel(),
  ];
}

function cardNode(c: CoinCard, href: string): HTMLElement {
  if (c.source === "sample") return el("div", { class: "coin-card card" }, ...cardBody(c));
  return el("a", { class: "coin-card card coin-card-open", href: screenLink(href, "coin", { coin: c.account }) }, ...cardBody(c));
}

function topNode(c: CoinCard, href: string): HTMLElement {
  const inner: Child[] = [
    drawCoinTile({ account: c.account, ticker: c.ticker, icon: c.icon, small: true }),
    el(
      "div",
      { class: "top-coin-text" },
      el("div", { class: "top-coin-name" }, el("b", {}, c.name), " ", el("span", { class: "num top-coin-ticker" }, c.ticker)),
      el("span", { class: "num top-coin-cap" }, c.marketCap),
      el("span", { class: "num top-coin-change" }, c.change ? `Trades ${c.change}` : "Fewer than 2 trades"),
      c.source === "sample" && sampleLabel("Sample data"),
    ),
  ];
  return c.source === "sample" ? el("div", { class: "top-coin" }, ...inner) : el("a", { class: "top-coin", href: screenLink(href, "coin", { coin: c.account }) }, ...inner);
}

const HOW_STEPS = [
  "Every coin has the same sell rules, fixed at launch.",
  "The creator, early buyers and large holders sell one allowance per hour. A bigger sell fills in part.",
  "Each limited sell shows as a receipt: asked, filled, and when more can follow.",
];

/** Draws the list once and returns a function that swaps in new cards without drawing the page again. */
function drawList(app: HTMLElement, href: string, firstCards: CoinCard[], note: string): (cards: CoinCard[], note: string) => void {
  let cards = firstCards;
  const pick: { tab: ListTab; query: string } = { tab: "Market cap", query: "" };
  const search = el("input", { id: "coin-search", type: "search", placeholder: "Name, ticker or coin account", autocomplete: "off" });
  const tabs = el("div", { role: "tablist", "aria-label": "Coin list", class: "list-tabs" });
  const grid = el("div", { class: "coin-grid", id: "coin-list" });
  const empty = el("div", { class: "card", role: "status", id: "list-empty" });
  const tabNote = el("p", { class: "muted", id: "tab-note" });
  // Where the list comes from, as one small line at the foot of the page, as in the design.
  const noteLine = el("p", { class: "list-source", id: "list-note" }, note);
  const top = el("section", { class: "top-coins", "aria-label": "Top coins by market cap" });

  function refresh(): void {
    const m = coinListModel(cards, pick);
    tabs.replaceChildren(
      ...m.tabs.map((t) => {
        const b = el("button", { type: "button", role: "tab", class: "list-tab", "aria-selected": String(t.label === pick.tab) }, `${t.label} `, el("span", { class: "num" }, String(t.count)));
        b.addEventListener("click", () => {
          pick.tab = t.label;
          refresh();
        });
        return b;
      }),
    );
    top.replaceChildren(...m.top.map((c) => topNode(c, href)));
    top.hidden = m.top.length === 0;
    tabNote.textContent = m.tabNote ?? "";
    tabNote.hidden = m.tabNote === null;
    empty.textContent = m.emptyText ?? "";
    empty.hidden = m.emptyText === null;
    grid.replaceChildren(...m.visible.map((c) => cardNode(c, href)));
  }
  search.addEventListener("input", () => {
    pick.query = search.value;
    refresh();
  });
  app.replaceChildren(
    screenHeader(href, "coins"),
    el("div", { class: "list-title" }, el("div", {}, el("h1", { id: "coins-title" }, "Coins"), el("p", {}, "Each card shows its cap state and latest receipt.")), el("a", { class: "button button-dark", id: "launch-coin", href: screenLink(href, "launch") }, "Launch a coin")),
    top,
    el(
      "section",
      { class: "card how-strip", "aria-label": "How it works" },
      el("ol", {}, ...HOW_STEPS.map((t, i) => el("li", {}, el("span", { class: "step-n" }, String(i + 1)), el("span", {}, t)))),
      el("a", { class: "text-link", href: `${screenLink(href, "landing")}#how` }, "How it works"),
    ),
    el("div", { class: "search-field" }, el("label", { for: "coin-search" }, "Search coins"), search),
    tabs,
    tabNote,
    empty,
    grid,
    noteLine,
  );
  refresh();
  return (next, nextNote) => {
    cards = next;
    noteLine.textContent = nextNote;
    refresh();
  };
}

function chainNote(link: Extract<ListLink, { kind: "chain" }>, cards: CoinCard[]): string {
  return `Read from ${link.source}, factory ${link.factory}. ${cards.length === 0 ? "" : `${cards.length} coin${cards.length === 1 ? "" : "s"}.`}`;
}

let stopRefresh: (() => void) | null = null;

async function load(app: HTMLElement, href: string, link: ListLink): Promise<void> {
  if (link.kind !== "chain") {
    const { cards, note } = fallbackList(link, new URL(href).hostname);
    drawList(app, href, cards, note);
    return;
  }
  app.replaceChildren(screenHeader(href, "coins"), el("h1", { id: "coins-title" }, "Coins"), el("p", { class: "status", role: "status", id: "list-status" }, "Reading the chain."));
  stopRefresh?.();
  stopRefresh = null;
  const readCards = async (): Promise<CoinCard[]> => {
    const nowNs = BigInt(Date.now()) * 1_000_000n;
    const fastNear = link.fastNearUrl ? { fastNearUrl: link.fastNearUrl } : {};
    const factory = chainReader({ rpcUrl: link.rpcUrl, coin: link.factory, anyLoggedTx: true, ...fastNear });
    return loadCoinCards(factory, link.fromBlock, (coin) => chainReader({ rpcUrl: link.rpcUrl, coin, ...fastNear }), { nowNs });
  };
  try {
    const cards = await readCards();
    const update = drawList(app, href, cards, chainNote(link, cards));
    // Later reads swap the cards in place. A failed read keeps the cards on screen and says so.
    const stop = refreshWhileShown(
      async () => {
        const next = await readCards();
        update(next, chainNote(link, next));
      },
      (_e, retrying) => {
        const line = document.querySelector("#list-note");
        if (line) line.textContent = retrying ? `Could not refresh from ${link.source}. Trying again.` : `Could not refresh from ${link.source}. Reload the page to try again.`;
      },
    );
    stopRefresh = stop.stop;
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    const retry = el("button", { type: "button", class: "button" }, "Try again");
    retry.addEventListener("click", () => void load(app, href, link));
    app.replaceChildren(
      screenHeader(href, "coins"),
      el("h1", { id: "coins-title" }, "Coins"),
      el("div", { role: "alert", class: "banner banner-error", id: "list-error" }, el("span", { class: "banner-grow" }, el("b", {}, `The coin list cannot load from ${link.source}.`), ` ${message}`), retry),
    );
  }
}

export function drawCoinList(app: HTMLElement, href: string): void {
  let link: ListLink;
  try {
    link = resolveListLink(href);
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    app.replaceChildren(screenHeader(href, "coins"), el("h1", { id: "coins-title" }, "Coins"), el("div", { role: "alert", class: "banner banner-error", id: "list-error" }, el("b", {}, "The coin list cannot load."), ` ${message}`));
    return;
  }
  void load(app, href, link);
}
