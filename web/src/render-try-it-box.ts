// Draws the "How to try it" steps, and the How to try screen that holds them. Text goes in as text nodes.
import { el } from "./dom-elements.ts";
import { screenHeader } from "./screen-chrome.ts";
import { FAUCET_LINK_TEXT, FAUCET_URL, TRY_IT_NOTE, TRY_IT_TITLE, tryItSteps } from "./try-it-copy.ts";

export function drawTryItBox(): HTMLElement {
  const [wallet, faucet, use] = tryItSteps();
  const faucetLink = el("a", { class: "text-link", href: FAUCET_URL, target: "_blank", rel: "noopener noreferrer" }, FAUCET_LINK_TEXT);
  return el(
    "section",
    { id: "try-it", class: "card try-it", "aria-labelledby": "try-it-h" },
    el("h2", { id: "try-it-h" }, TRY_IT_TITLE),
    el("ol", {}, el("li", {}, wallet), el("li", {}, faucet, " ", faucetLink), el("li", {}, use)),
    el("p", { class: "muted" }, TRY_IT_NOTE),
  );
}

/** The How to try screen: the header and the steps. */
export function drawTryScreen(app: HTMLElement, href: string): void {
  app.replaceChildren(screenHeader(href, "try"), drawTryItBox());
}
