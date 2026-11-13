// Browser entry: loads the coin page from the page URL and draws it. All text comes from the
// page model; this file only builds elements. It never sets innerHTML, so chain text cannot
// become markup.
import { loadCoinPage, type CoinPageData } from "./coin-page.ts";
import { el, type Child } from "./dom-elements.ts";
import { drawYourPlan } from "./render-your-plan.ts";
import {
  coinPageModel,
  type Banner,
  type Card,
  type CoinPageModel,
  type CoinPageView,
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

function drawReady(m: CoinPageModel, plan: HTMLElement | null): HTMLElement[] {
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
          el("a", { class: "chip chip-link", href: "#rules" }, "Rules"),
        ),
        h.description && el("p", { class: "coin-description", id: "coin-description" }, h.description),
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
  nodes.push(
    el(
      "div",
      { class: "summary-lines card", id: "summary-lines" },
      ...m.summaryLines.map((l) => el("div", { class: "summary-line" }, el("b", {}, l.label), el("span", { class: `num tone-${l.tone}` }, l.value))),
    ),
  );
  nodes.push(section("Who can sell how fast", "who-can-sell", el("h2", {}, "Who can sell how fast"), el("div", { class: "who-grid" }, ...m.who.map(drawCard))));
  nodes.push(plan);
  nodes.push(
    section(
      "Receipts",
      "receipts-section",
      el("div", { class: "section-head" }, el("h2", {}, "Receipts"), el("span", { class: "muted" }, "Read from the chain. Newest at the top.")),
      m.receiptsEmpty && el("p", { class: "note", id: "receipts-empty" }, m.receiptsEmpty),
      el("ol", { id: "receipts" }, ...m.receipts.map(drawReceipt)),
    ),
  );
  nodes.push(
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
  return nodes.filter((n): n is HTMLElement => n !== null);
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
  ];
}

function draw(view: CoinPageView, retry: () => void, data: CoinPageData | null): void {
  const status = el("p", { id: "status", class: "status", role: "status" }, view.status);
  const body = view.state === "ready" ? drawReady(view, data?.kind === "ready" ? drawYourPlan(data.reads) : null) : drawError(view);
  app.replaceChildren(...body.slice(0, 1), status, ...body.slice(1));
  app.querySelector("#load-error button")?.addEventListener("click", retry);
  const marked = app.querySelector(".from-alert");
  if (marked) marked.scrollIntoView({ block: "center" });
}

function drawLoading(): void {
  app.replaceChildren(
    el("h1", { id: "coin" }, "Fair Chance"),
    el("p", { id: "status", class: "status", role: "status" }, "Reading the chain."),
    el("p", { class: "note" }, "Loading."),
  );
}

function load(): void {
  drawLoading();
  const nowNs = BigInt(Date.now()) * 1_000_000n;
  loadCoinPage(location.href, { nowNs })
    .then((data) => draw(coinPageModel(data, { nowNs }), load, data))
    .catch((err: unknown) => {
      // loadCoinPage does not throw; this only guards the drawing code.
      const message = err instanceof Error ? err.message : String(err);
      draw({ state: "error", status: `Values cannot load: ${message}`, title: "Values cannot load from the chain.", detail: message, lastAttempt: "" }, load, null);
    });
}

load();
