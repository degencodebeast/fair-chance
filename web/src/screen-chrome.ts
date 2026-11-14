// The header the landing, coin list and alerts screens share: the name, the three screens and the
// Open app button on the right.
import { el } from "./dom-elements.ts";
import { listen } from "./dom-listen.ts";
import { screenLink, type Screen } from "./screen-route.ts";
import type { WalletControl, WalletState } from "./wallet-session.ts";

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

/**
 * The wallet part of the header: "Connect wallet" when signed out, else the account and "Sign out".
 * A refused account shows its one sentence. It redraws on each change of the wallet state, and
 * stops when the page takes it out of the document.
 */
export function walletBar(control: WalletControl): HTMLElement {
  const root = el("div", { id: "wallet-bar", class: "wallet-bar" });
  let connecting = false;
  let shown = false;

  const draw = (state: WalletState): void => {
    if (state.kind === "signed-in") {
      const out = el("button", { type: "button", id: "wallet-sign-out", class: "button" }, "Sign out");
      listen(out, "click", () => void control.signOut());
      root.replaceChildren(el("span", { id: "wallet-account", class: "wallet-account" }, state.session.accountId), out);
      return;
    }
    const connect = el("button", { type: "button", id: "wallet-connect", class: "button button-dark" }, connecting ? "Connecting" : "Connect wallet");
    if (connecting) connect.setAttribute("disabled", "");
    listen(connect, "click", () => {
      if (connecting) return;
      connecting = true;
      draw(control.state());
      void control.connect().finally(() => {
        connecting = false;
        draw(control.state());
      });
    });
    root.replaceChildren(connect);
    if (state.notice !== null) root.append(el("span", { id: "wallet-notice", class: "muted", role: "status" }, state.notice));
  };

  const stop = control.subscribe((state) => {
    if (root.isConnected) shown = true;
    else if (shown) return stop();
    if (!connecting) draw(state);
  });
  draw(control.state());
  return root;
}
