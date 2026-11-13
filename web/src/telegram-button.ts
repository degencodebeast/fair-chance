// The "Watch on Telegram" control. It is turned off, with its reason beside it, until a public bot is approved.
import { el } from "./dom-elements.ts";

export function telegramButton(telegram: { label: string; reason: string }): HTMLElement {
  return el(
    "div",
    { class: "telegram" },
    el("button", { type: "button", class: "button button-off", id: "watch-telegram", disabled: "", "aria-describedby": "watch-telegram-reason" }, telegram.label),
    el("span", { class: "muted", id: "watch-telegram-reason" }, telegram.reason),
  );
}
