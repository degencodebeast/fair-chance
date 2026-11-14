// The "Open the bot on Telegram" control: a link to the public alert bot, with the sentence that
// says what to send it. The link, its text and the sentence are fixed here, so no page link changes them.
import { el } from "./dom-elements.ts";

export const BOT_URL = "https://t.me/fairchance_alert_bot";
export const BOT_LINK_TEXT = "Open the bot on Telegram";
export const BOT_SENTENCE = "Send /check <coin> for its rules. Alerts go to the coin's team only for now.";

export function telegramButton(telegram: { label: string; reason: string }): HTMLElement {
  return el(
    "div",
    { class: "telegram" },
    el("a", { class: "button", id: "watch-telegram", href: BOT_URL, target: "_blank", rel: "noopener noreferrer", "aria-describedby": "watch-telegram-reason" }, telegram.label),
    el("span", { class: "muted", id: "watch-telegram-reason" }, telegram.reason),
  );
}
