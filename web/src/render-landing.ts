// Draws the landing screen from landing-copy.ts. Text goes in as text nodes only.
import { el, type Child } from "./dom-elements.ts";
import { landingCopy as copy } from "./landing-copy.ts";
import { screenHeader } from "./screen-chrome.ts";
import { screenLink } from "./screen-route.ts";

function itemCard(id: string, block: { kicker: string; title: string; items: { b: string; t: string }[]; note?: string }): HTMLElement {
  return el(
    "section",
    { class: "card landing-card", "aria-labelledby": id },
    el("div", { class: "kicker" }, block.kicker),
    el("h2", { id }, block.title),
    el("div", { class: "landing-items" }, ...block.items.map((x) => el("p", { class: "landing-item" }, el("b", {}, `${x.b} `), x.t))),
    block.note ? el("p", { class: "muted" }, block.note) : null,
  );
}

export function drawLanding(app: HTMLElement, href: string): void {
  const coins = screenLink(href, "coins");
  const r = copy.receiptCard;
  const receipt = el(
    "aside",
    { class: "hero-receipt", id: "hero-receipt", "aria-label": "Sell receipt from the CABAL2 testnet run" },
    el("div", { class: "hr-head" }, el("b", {}, r.brand), el("span", { class: "hr-tag" }, r.tag)),
    el("p", { class: "hr-meta" }, r.meta),
    el("p", { class: "hr-who" }, r.who),
    el(
      "div",
      { class: "hr-allowance" },
      el("div", { class: "hr-line" }, el("span", {}, r.allowance.label), el("span", { class: "num" }, r.allowance.value)),
      el("div", { class: "meter" }, el("div", { class: "meter-used", style: `width:${r.allowance.pct}%` })),
    ),
    ...r.rows.map((x) => el("div", { class: "hr-row" }, el("span", {}, x.k), el("span", { class: "hr-dots" }), el("span", { class: "num" }, x.v))),
    el(
      "div",
      { class: "hr-highlight" },
      el("div", {}, el("b", {}, r.highlight.label), el("p", { class: "hr-sub" }, r.highlight.sub)),
      el("span", { class: "hr-big num" }, r.highlight.value),
    ),
    el("p", { class: "hr-foot" }, r.reason),
    el("p", { class: "hr-foot" }, r.foot),
  );
  const children: Child[] = [
    screenHeader(href, "landing"),
    el(
      "div",
      { class: "hero-wrap" },
      el(
      "section",
      { "aria-label": "Intro", class: "hero" },
      el("h1", { id: "landing-headline" }, copy.headline),
      el("p", { class: "hero-sub" }, copy.subheadline),
      el("p", { class: "hero-intro" }, copy.intro),
      el(
        "div",
        { class: "hero-actions" },
        copy.demoUrl !== null
          ? el("a", { class: "button button-dark button-big", id: "watch-demo", href: copy.demoUrl, target: "_blank", rel: "noopener noreferrer" }, copy.actions[0].label)
          : el("a", { class: "button button-dark button-big", id: "open-app", href: coins }, copy.openAppLabel),
        el("a", { class: "button button-big", id: "launch-coin", href: screenLink(href, "launch") }, copy.actions[1].label),
        el("a", { class: "text-link", id: "see-rules", href: `${screenLink(href, "coin", { coin: copy.proof.coin })}#rules` }, copy.actions[2].label),
      ),
      el(
        "div",
        { class: "proof", id: "proof" },
        el("p", { class: "proof-source" }, copy.proof.source),
        el("p", {}, copy.proof.sentence),
        el("a", { class: "text-link text-link-light", href: screenLink(href, "coin", { coin: copy.proof.coin }) }, copy.proof.link),
      ),
    ),
      receipt,
    ),
    el("section", { "aria-label": "The problem", class: "problem" }, el("p", {}, copy.problem)),
    itemCard("buyers-h", copy.buyers),
    itemCard("creators-h", copy.creators),
    el(
      "section",
      { "aria-labelledby": "how-h", id: "how" },
      el("h2", { id: "how-h" }, copy.how.title),
      el("ol", { class: "how-steps" }, ...copy.how.steps.map((s) => el("li", {}, el("span", { class: "step-n" }, s.n), el("span", {}, el("b", {}, `${s.b} `), s.t)))),
    ),
    el(
      "section",
      { "aria-labelledby": "wont-h", class: "wont" },
      el("h2", { id: "wont-h" }, copy.wontTitle),
      el("p", {}, copy.wontLead),
      el("ul", {}, ...copy.wont.map((w) => el("li", {}, w))),
      el("a", { class: "text-link text-link-light", id: "known-limits", href: `${copy.githubUrl}#known-limits`, target: "_blank", rel: "noopener noreferrer" }, copy.wontLink),
    ),
    el("footer", { class: "landing-footer" }, `${copy.footer} `, el("a", { class: "text-link", href: copy.githubUrl, target: "_blank", rel: "noopener noreferrer" }, copy.githubLabel)),
  ];
  app.replaceChildren(...(children.filter(Boolean) as Node[]));
}
