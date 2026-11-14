// Browser entry: loads the coin page from the page URL and draws it. All text comes from the
// page model; this file only builds elements. It never sets innerHTML, so chain text cannot
// become markup.
import { loadCoinPage, type CoinPageData, type CoinReads } from "./coin-page.ts";
import { el, type Child } from "./dom-elements.ts";
import { telegramButton } from "./telegram-button.ts";
import { chainReader, configFromUrl } from "./chain.ts";
import { drawPriceChart } from "./render-price-chart.ts";
import { fillHolders, holdersPlaceholder } from "./render-holders-table.ts";
import { drawYourPlan } from "./render-your-plan.ts";
import { refreshWhileShown } from "./refresher.ts";
import { loadBuyQuote } from "./buy-quote.ts";
import { claimPanelModel } from "./claim-panel-model.ts";
import { drawClaimPanel } from "./render-claim-panel.ts";
import { drawTradeBox } from "./render-trade-box.ts";
import {
  coinPageModel,
  type Banner,
  type Card,
  type CoinPageModel,
  type CoinPageView,
  type PlaceholderFrame,
  loadingFrame,
  type ReceiptItem,
} from "./coin-page-model.ts";

const app = document.querySelector<HTMLElement>("#app")!;

function section(label: string, id: string, ...children: Child[]): HTMLElement {
  return el("section", { id, "aria-label": label, class: "card" }, ...children);
}

function drawBanner(b: Banner): HTMLElement {
  if (b.kind === "early") {
    return el(
      "div",
      { role: "status", class: "banner banner-early", id: "early-banner" },
      el("div", { class: "banner-count" }, el("span", { class: "num-big" }, String(b.blocksLeft)), el("span", {}, "blocks left in the early window")),
      el("div", { class: "banner-facts" }, el("span", {}, `Block ${b.block} of ${b.end}`), el("span", {}, `Buy fee now ${b.buyFee}`), el("span", {}, b.feeEnds)),
      el("p", {}, b.text),
    );
  }
  if (b.kind === "old") return el("div", { role: "status", class: "banner banner-ended", id: "early-banner" }, el("b", {}, b.text));
  return el("div", { role: "status", class: "banner banner-ended", id: "early-banner" }, el("b", {}, `The early window ended at block ${b.end}.`), el("span", {}, ` ${b.text}`));
}

function drawCard(c: Card): HTMLElement {
  return el(
    "div",
    { class: "who-card", "data-card": c.id },
    el("div", { class: "who-title" }, el("b", {}, c.title), c.chip && el("span", { class: `chip chip-${c.chip.tone}` }, c.chip.text)),
    ...c.lines.map((l) => el("p", { class: `who-line tone-${l.tone}` }, l.text)),
    c.meter &&
      el(
        "div",
        { class: "meter-block" },
        el("p", { class: "meter-text" }, c.meter.text),
        el("div", { class: "meter" }, el("div", { class: `meter-used meter-${c.meter.tone}`, style: `width:${Math.min(100, c.meter.usedPct)}%` })),
      ),
  );
}

function drawReceipt(r: ReceiptItem): HTMLElement {
  const attrs: Record<string, string> = {
    "data-event-receipt-id": r.eventReceiptId,
    "data-block": String(r.block),
    class: r.fromAlert ? "receipt from-alert" : "receipt",
  };
  // Only a sell is a sell receipt; the browser read in the verify run looks for these.
  if (r.sellReceiptId !== null) attrs["data-receipt-id"] = r.sellReceiptId;
  return el(
    "li",
    attrs,
    el("span", { class: "receipt-time" }, r.time ?? `Block ${r.block}`),
    el(
      "div",
      { class: "receipt-body" },
      el("div", { class: "receipt-kind" }, el("b", {}, r.kind), r.fromAlert && el("span", { class: "chip chip-limit" }, "From your alert")),
      el("p", { class: "receipt-sentence" }, r.sentence),
      r.detail && el("p", { class: `receipt-detail tone-${r.tone}` }, r.detail),
    ),
  );
}

interface ReadyParts {
  plan: HTMLElement | null;
  chart: HTMLElement | null;
  holders: HTMLElement | null;
  trade: HTMLElement | null;
  claim: HTMLElement | null;
}

function drawReady(m: CoinPageModel, parts: ReadyParts): HTMLElement[] {
  const { plan, chart, holders, trade, claim } = parts;
  const h = m.header;
  const nodes: (HTMLElement | null)[] = [];
  nodes.push(
    el(
      "header",
      { class: "coin-header" },
      h.icon ? el("img", { class: "coin-icon", src: h.icon, alt: "", width: "60", height: "60" }) : el("div", { class: "coin-icon coin-icon-empty", "aria-hidden": "true" }),
      el(
        "div",
        { class: "coin-title" },
        el("div", { class: "coin-name" }, el("h1", { id: "coin" }, h.name), h.symbol && el("span", { class: "num" }, h.symbol)),
        el(
          "div",
          { class: "chips" },
          el("span", { class: "chip", id: "coin-account" }, new URL(location.href).searchParams.get("coin") ?? ""),
          el("span", { class: "chip" }, `Creator ${h.creator}`),
          el("span", { class: "chip" }, `Launched ${h.launched}`),
          el("a", { class: "chip chip-link chip-desktop", href: "#rules" }, h.rulesChip.desktop),
          el("a", { class: "chip chip-link chip-phone", href: "#rules" }, h.rulesChip.phone),
        ),
        h.description && el("p", { class: "coin-description", id: "coin-description" }, h.description),
        telegramButton(m.telegram),
      ),
    ),
  );
  nodes.push(
    m.warning &&
      el(
        "div",
        { role: "alert", class: "banner banner-warning", id: "chain-warning" },
        el("b", {}, m.warning.title),
        el("ul", {}, ...m.warning.reasons.map((r) => el("li", {}, r))),
      ),
  );
  nodes.push(m.banner && drawBanner(m.banner));
  nodes.push(
    el(
      "div",
      { class: "stats", id: "stats" },
      ...m.stats.map((s) =>
        el(
          "div",
          { class: "stat" },
          el("div", { class: "stat-label" }, s.label),
          el("div", { class: s.available ? "stat-value" : "stat-value stat-missing" }, s.value),
          el("div", { class: "stat-sub" }, s.sub),
        ),
      ),
    ),
  );
  const grid: (HTMLElement | null)[] = [];
  grid.push(
    el(
      "div",
      { class: "summary-lines card", id: "summary-lines" },
      ...m.summaryLines.map((l) => el("div", { class: "summary-line" }, el("b", {}, l.label), el("span", { class: `num tone-${l.tone}` }, l.value))),
    ),
  );
  grid.push(claim);
  grid.push(section(
      "Who can sell how fast",
      "who-can-sell",
      el("div", { class: "section-head" }, el("h2", {}, "Who can sell how fast"), el("span", { class: "as-of num muted", id: "as-of" }, m.asOf)),
      el("div", { class: "who-grid" }, ...m.who.map(drawCard))));
  grid.push(plan);
  grid.push(trade);
  grid.push(
    section(
      "Receipts",
      "receipts-section",
      el("div", { class: "section-head" }, el("h2", {}, "Receipts"), el("span", { class: "muted" }, "Read from the chain. Newest at the top.")),
      m.receiptsEmpty && el("p", { class: "note", id: "receipts-empty" }, m.receiptsEmpty),
      el("ol", { id: "receipts" }, ...m.receipts.map(drawReceipt)),
    ),
  );
  grid.push(chart);
  grid.push(holders);
  grid.push(
    section(
      "Rules",
      "rules",
      el("h2", {}, "Rules"),
      el("p", { class: "muted" }, "The same for every coin. These are facts of the coin, not settings."),
      el(
        "dl",
        { class: "rules-grid" },
        ...m.rules.flatMap((r) => [el("dt", {}, r.label), el("dd", {}, r.value)]),
        el("dt", {}, "Code hash"),
        el("dd", { id: "code-hash" }, m.codeHash),
      ),
      m.rulesStatement && el("p", { class: "statement", id: "rules-statement" }, m.rulesStatement),
      el("h3", {}, "Coin account checks"),
      el("p", { class: "muted", id: "checks-source" }, m.checksSource),
      el(
        "ul",
        { class: "checks", id: "chain-checks" },
        ...m.checks.map((c) => el("li", { "data-check": c.id, "data-status": c.status }, el("b", {}, `${c.label}: ${c.status === "pass" ? "yes" : c.status === "fail" ? "no" : "not checked"}. `), c.detail)),
      ),
      el("h3", {}, "What these rules do not stop"),
      el("ul", { class: "gaps" }, ...m.gaps.map((g) => el("li", {}, g))),
      el("p", { class: "legend" }, el("span", { class: "swatch", "aria-hidden": "true" }), "Magenta means a limit is in force."),
    ),
  );
  nodes.push(el("div", { class: "coin-page-grid" }, ...grid.filter((n): n is HTMLElement => n !== null)));
  return nodes.filter((n): n is HTMLElement => n !== null);
}

function drawFrame(f: PlaceholderFrame): HTMLElement[] {
  return [
    el(
      "div",
      { class: "stats", id: "stats" },
      ...f.stats.map((x) => el("div", { class: "stat" }, el("div", { class: "stat-label" }, x.label), el("div", { class: "stat-value stat-missing" }, x.value), el("div", { class: "stat-sub" }, ""))),
    ),
    el(
      "div",
      { class: "summary-lines card", id: "summary-lines" },
      ...f.summaryLines.map((l) => el("div", { class: "summary-line" }, el("b", {}, l.label), el("span", { class: "num" }, l.value))),
    ),
    section(
      "Who can sell how fast",
      "who-can-sell",
      el("div", { class: "section-head" }, el("h2", {}, "Who can sell how fast")),
      el("div", { class: "who-grid" }, ...f.who.map((c) => el("div", { class: "who-card", "data-card": "none" }, el("div", { class: "who-title" }, el("b", {}, c.title)), el("p", { class: "who-line" }, c.text)))),
    ),
    section(
      "Receipts",
      "receipts-section",
      el("div", { class: "section-head" }, el("h2", {}, "Receipts")),
      el("p", { class: "note", id: "receipts-empty" }, f.receipts),
    ),
  ];
}

function drawError(view: Extract<CoinPageView, { state: "error" }>): HTMLElement[] {
  return [
    el("h1", { id: "coin" }, "Fair Chance"),
    el(
      "div",
      { role: "alert", class: "banner banner-error", id: "load-error" },
      el("span", { class: "banner-grow" }, el("b", {}, view.title), ` ${view.detail} Last attempt ${view.lastAttempt}.`),
      el("button", { type: "button", class: "button" }, "Try again"),
    ),
    ...drawFrame(view.frame),
  ];
}

let refreshTimer: ReturnType<typeof setTimeout> | undefined;

/** Reads the chain again when the cap hour or the early block count ends, so no old number stays as live. */
function scheduleRefresh(view: CoinPageView, reload: () => void): void {
  clearTimeout(refreshTimer);
  if (view.state !== "ready" || view.refreshAtNs === null) return;
  const waitMs = Number((view.refreshAtNs - BigInt(Date.now()) * 1_000_000n) / 1_000_000n) + 500;
  refreshTimer = setTimeout(reload, Math.min(Math.max(waitMs, 500), 2_147_483_647));
}

function tradeBox(reads: CoinReads, symbol: string): HTMLElement {
  const chain = chainReader(configFromUrl(location.href));
  const decimals = reads.metadata.kind === "value" ? reads.metadata.value.decimals : 18;
  return drawTradeBox({ symbol, decimals, largeThresholdBps: reads.rules.largeThresholdBps, storageMin: reads.storageMin, loadQuote: (accountId, nearIn) => loadBuyQuote(chain, accountId, nearIn) });
}

function claimPanel(reads: CoinReads): HTMLElement {
  return drawClaimPanel(claimPanelModel({ fees: reads.creatorFees, claimMinRealNear: reads.rules.claimMinRealNear, claimDepthHoldS: reads.rules.claimDepthHoldS, nowNs: reads.loadedAtNs }));
}

/** What a visitor has typed or opened in a section; a refresh keeps such a section as it is. */
function holdsVisitorWork(node: HTMLElement | null): boolean {
  if (node === null) return false;
  if (node.contains(document.activeElement)) return true;
  return [...node.querySelectorAll("input, textarea")].some((i) => (i as HTMLInputElement).value !== "");
}

interface Drawn {
  plan: HTMLElement | null;
  trade: HTMLElement | null;
  holders: HTMLElement | null;
  eventCount: number;
}
let drawn: Drawn | null = null;

function draw(view: CoinPageView, retry: () => void, data: CoinPageData | null, keep = false): void {
  scheduleRefresh(view, retry);
  const status = el("p", { id: "status", class: "status", role: "status" }, view.status);
  const reads = data?.kind === "ready" ? data.reads : null;
  const symbol = reads?.metadata.kind === "value" ? reads.metadata.value.symbol : "tokens";
  const chart = reads ? drawPriceChart(reads.events, symbol) : null;
  // On a refresh the holders table stays unless there is new activity, and a form with the visitor's input stays as it is.
  const old = keep && reads ? drawn : null;
  const sameEvents = old !== null && reads !== null && old.eventCount === reads.events.length;
  const holders = reads ? (old?.holders && sameEvents ? old.holders : holdersPlaceholder()) : null;
  const plan = reads ? (old?.plan && holdsVisitorWork(old.plan) ? old.plan : drawYourPlan(reads)) : null;
  const trade = reads ? (old?.trade && holdsVisitorWork(old.trade) ? old.trade : tradeBox(reads, symbol)) : null;
  const parts = reads ? { plan, chart, holders, trade, claim: claimPanel(reads) } : { plan: null, chart, holders, trade: null, claim: null };
  drawn = reads && view.state === "ready" ? { plan, trade, holders, eventCount: reads.events.length } : null;
  const body = view.state === "ready" ? drawReady(view, parts) : drawError(view);
  app.replaceChildren(...body.slice(0, 1), status, ...body.slice(1));
  if (holders && reads && !(old?.holders === holders && sameEvents)) {
    const supply = reads.totalSupply.kind === "value" ? reads.totalSupply.value : null;
    void fillHolders(holders, chainReader(configFromUrl(location.href)), reads.events, supply, reads.loadedAtNs);
  }
  app.querySelector("#load-error button")?.addEventListener("click", retry);
  const marked = app.querySelector(".from-alert");
  if (marked) marked.scrollIntoView({ block: "center" });
}

function drawLoading(): void {
  app.replaceChildren(
    el("h1", { id: "coin" }, "Fair Chance"),
    el("p", { id: "status", class: "status", role: "status" }, "Reading the chain."),
    ...drawFrame(loadingFrame("Loading")),
  );
}

let loadingNow = false;
let refresh: { stop: () => void; restart: () => void } | null = null;

function load(): void {
  drawLoading();
  loadingNow = true;
  const nowNs = BigInt(Date.now()) * 1_000_000n;
  loadCoinPage(location.href, { nowNs })
    .then((data) => {
      draw(coinPageModel(data, { nowNs }), load, data);
      // A new read, by the first load, the Try again button or the cap hour, starts the refresh again.
      refresh?.restart();
    })
    .catch((err: unknown) => {
      // loadCoinPage does not throw; this only guards the drawing code.
      const message = err instanceof Error ? err.message : String(err);
      draw({ state: "error", status: `Values cannot load: ${message}`, title: "Values cannot load from the chain.", detail: message, lastAttempt: "", frame: loadingFrame("Not loaded") }, load, null);
    })
    .finally(() => {
      loadingNow = false;
    });
}

/**
 * Reads the coin again without the loading frame. A failed read leaves the page as it is and says so.
 * When the earlier load failed, a good read draws the whole page; a failed one keeps the error view.
 */
async function refreshOnce(): Promise<void> {
  if (loadingNow) return;
  const wasReady = drawn !== null;
  const nowNs = BigInt(Date.now()) * 1_000_000n;
  const data = await loadCoinPage(location.href, { nowNs });
  if (data.kind === "error") {
    if (wasReady) {
      const status = app.querySelector("#status");
      if (status) status.textContent = "Could not refresh.";
    } else {
      draw(coinPageModel(data, { nowNs }), load, data);
    }
    throw data.failure ?? new Error(data.message);
  }
  draw(coinPageModel(data, { nowNs }), load, data, wasReady);
}

load();
refresh = refreshWhileShown(refreshOnce, (_e, retrying) => {
  const status = app.querySelector("#status");
  if (status && drawn !== null) status.textContent = retrying ? "Could not refresh. Trying again." : "Could not refresh. Reload the page to try again.";
});
