// The header the landing, coin list and alerts screens share: the name, the three screens and the
// Open app button on the right.
import { el } from "./dom-elements.ts";
import { screenLink, type Screen } from "./screen-route.ts";

export function screenHeader(href: string, current: Screen): HTMLElement {
  const link = (screen: Screen, label: string) =>
    el("a", { class: "nav-link", href: screenLink(href, screen), ...(screen === current ? { "aria-current": "page" } : {}) }, label);
  return el(
    "header",
    { class: "screen-header" },
    el("a", { class: "brand", href: screenLink(href, "landing") }, "Fair Chance"),
    el("nav", { "aria-label": "Screens", class: "screen-nav" }, link("coins", "Coins"), link("alerts", "Alerts")),
    el("a", { class: "button button-dark", id: "header-open-app", href: screenLink(href, "coins") }, "Open app"),
  );
}

/** A control that cannot work yet: turned off, with the reason in view next to it. */
export function turnedOffButton(id: string, label: string, reason: string): HTMLElement {
  return el(
    "span",
    { class: "off-control" },
    el("button", { type: "button", id, class: "button button-off", disabled: "", "aria-describedby": `${id}-reason` }, label),
    el("span", { id: `${id}-reason`, class: "muted" }, reason),
  );
}
