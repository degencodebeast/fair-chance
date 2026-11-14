// The creator fee claim panel. The claim button is turned off, with its reason beside it. Text
// comes from the claim panel model.
import type { ClaimPanelModel } from "./claim-panel-model.ts";
import { el } from "./dom-elements.ts";
import { turnedOffButton } from "./screen-chrome.ts";
import type { WalletControl } from "./wallet-session.ts";

export function drawClaimPanel(m: ClaimPanelModel): HTMLElement {
  return el(
    "section",
    { id: "creator-fees", "aria-label": "Creator fee claim", class: "card claim-panel" },
    el(
      "div",
      { class: "claim-head" },
      el("h2", {}, "Creator fees"),
      m.amount && el("span", { id: "creator-fees-amount", class: "num claim-amount" }, m.amount),
    ),
    el("p", { id: "creator-fees-message" }, m.message),
    m.progress &&
      el(
        "div",
        { class: "claim-progress" },
        el("div", { class: "meter" }, el("div", { class: "meter-used meter-plain", style: `width:${m.progress.pct}%` })),
        el("span", { id: "creator-fees-progress", class: "num muted" }, m.progress.text),
      ),
    m.viewer && el("p", { class: "muted", id: "creator-fees-viewer" }, m.viewer),
    turnedOffButton("claim-fees-button", m.button.label, m.button.reason),
    m.refusals.notReady && el("p", { class: "muted", id: "creator-fees-too-early" }, `If the creator claims too early: ${m.refusals.notReady}`),
  );
}

/**
 * The full panel for the connected creator account, and one short line for everyone else. Both
 * follow the wallet. Nothing shows when the coin has no creator fee view.
 */
export function drawCreatorFees(m: ClaimPanelModel, wallet: WalletControl): { panel: HTMLElement | null; line: HTMLElement | null } {
  if (m.creator === null || m.line === null) return { panel: null, line: null };
  const panel = drawClaimPanel(m);
  const line = el("p", { id: "creator-fees-line", class: "muted creator-fees-line" }, m.line);
  const follow = (): void => {
    const s = wallet.state();
    const isCreator = s.kind === "signed-in" && s.session.accountId === m.creator;
    panel.hidden = !isCreator;
    line.hidden = isCreator;
  };
  let shown = false;
  const stop = wallet.subscribe(() => {
    if (panel.isConnected || line.isConnected) shown = true;
    else if (shown) return stop();
    follow();
  });
  follow();
  return { panel, line };
}
