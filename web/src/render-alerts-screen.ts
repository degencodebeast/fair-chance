// Draws the alerts screen: a chat with the watcher bot, the same messages as plain text, and the
// /check card. Every message is an example and the screen says so; the sentences are the watcher's.
import { el, type Child } from "./dom-elements.ts";
import { sampleLabel } from "./sample-label.ts";
import { screenHeader } from "./screen-chrome.ts";
import { watcherScreenModel, type ChatMessage } from "./watcher-screen-model.ts";

function openControl(m: ChatMessage): Node | null {
  if (m.receipt === null) return null;
  if (m.openHref !== null) return el("a", { class: "open-on-fair-chance", href: m.openHref }, "Open on Fair Chance");
  return el("button", { type: "button", class: "open-on-fair-chance", disabled: "", title: "Sample alert: no real receipt" }, "Open on Fair Chance");
}

export function drawAlertsScreen(app: HTMLElement, href: string): void {
  const m = watcherScreenModel(href);
  const bubbles = m.chat.map((c) =>
    el(
      "div",
      { class: `chat-bubble chat-${c.from}` },
      el("div", { class: "chat-text" }, c.text),
      openControl(c),
      el("div", { class: "chat-time" }, c.time),
    ),
  );
  const children: Child[] = [
    screenHeader(href, "alerts"),
    el("div", { class: "alerts-intro" }, el("h1", { id: "alerts-title" }, "Fair Chance alerts"), el("p", {}, "The watcher reads each coin's events and sends one fixed message per event on Telegram. It cannot move funds or change a rule, and it gives no advice. The contract enforces the limits. The coin page reads receipts from the chain and works without Telegram."), el("p", { class: "sample-label", id: "alerts-sample-label" }, m.sampleLabel)),
    m.openNote && el("p", { class: "note", id: "open-note" }, m.openNote),
    el(
      "div",
      { class: "alerts-grid" },
      el("section", { "aria-label": "Telegram chat", class: "chat" }, el("div", { class: "chat-title" }, "Fair Chance alerts"), el("div", { class: "chat-label" }, sampleLabel(m.chatLabel)), el("div", { class: "chat-body" }, ...bubbles)),
      el(
        "section",
        { "aria-label": "Plain text", class: "card" },
        el("h2", {}, "Plain text, as sent"),
        el("p", { class: "muted" }, "Fixed templates, one message per event."),
        sampleLabel(m.plainLabel),
        ...m.plain.map((p) => el("div", { class: "plain-item" }, el("div", { class: "plain-label" }, p.label), el("pre", {}, p.text))),
      ),
    ),
    el("section", { "aria-label": "Pre-buy card", class: "card", id: "pre-buy-card" }, el("h2", {}, "Pre-buy card"), sampleLabel(m.preBuyLabel), el("pre", {}, m.checkCard)),
  ];
  app.replaceChildren(...(children.filter(Boolean) as Node[]));
}
