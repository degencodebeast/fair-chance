// The trade box: a buy preview and a sell tab. Connect, buy and sell are turned off, each with its
// reason beside it; nothing here signs or sends. The only read is fc_quote_buy for the account and
// NEAR amount the visitor types. Text comes from the trade box model.
import type { BuyQuote } from "./buy-quote.ts";
import type { Reading } from "./coin-page.ts";
import { el } from "./dom-elements.ts";
import { listen } from "./dom-listen.ts";
import { turnedOffButton } from "./screen-chrome.ts";
import { parseTokenAmount } from "./token-amount.ts";
import { tradeBoxModel, type BuyPreview } from "./trade-box-model.ts";

const NEAR_DECIMALS = 24;

export interface TradeBoxDeps {
  symbol: string;
  decimals: number;
  largeThresholdBps: number;
  loadQuote: (accountId: string, nearIn: bigint) => Promise<Reading<BuyQuote>>;
}

export function drawTradeBox(deps: TradeBoxDeps): HTMLElement {
  const model = (preview: BuyPreview) => tradeBoxModel({ symbol: deps.symbol, decimals: deps.decimals, largeThresholdBps: deps.largeThresholdBps, preview });
  const first = model({ kind: "none" });

  const tabBuy = el("button", { type: "button", role: "tab", id: "trade-tab-buy", "aria-selected": "true", "aria-controls": "trade-buy-panel", class: "tab" }, "Buy");
  const tabSell = el("button", { type: "button", role: "tab", id: "trade-tab-sell", "aria-selected": "false", "aria-controls": "trade-sell-panel", class: "tab" }, "Sell");

  const account = el("input", { id: "trade-buy-account", name: "account", type: "text", autocomplete: "off", spellcheck: "false", placeholder: "you.testnet" });
  const amount = el("input", { id: "trade-buy-amount", name: "near", type: "text", inputmode: "decimal", autocomplete: "off", placeholder: "1.00" });
  const status = el("p", { id: "trade-buy-status", class: "muted", role: "status" }, "Type an account and an amount to preview a buy.");
  const lines = el("dl", { id: "trade-buy-lines", class: "preview-lines" });
  const noteSlot = el("div", { id: "trade-buy-note" });
  const warnSlot = el("div", { id: "trade-buy-warning-slot" });
  const buySlot = el("div", { id: "trade-buy-action" }, turnedOffButton("trade-buy-button", first.buy.button.label, first.buy.button.reason));
  const buyForm = el(
    "form",
    { id: "trade-buy-form", class: "plan-form" },
    el("label", { for: "trade-buy-account" }, "Account name"),
    account,
    el("label", { for: "trade-buy-amount" }, first.buy.amountLabel),
    amount,
    el("button", { type: "submit", id: "trade-buy-preview", class: "button button-dark" }, "Preview buy"),
  );

  const show = (preview: BuyPreview): void => {
    const m = model(preview);
    lines.replaceChildren(...m.buy.lines.flatMap((l) => [el("dt", {}, l.label), el("dd", {}, l.value)]));
    noteSlot.replaceChildren(...(m.buy.note ? [el("p", { class: "note" }, m.buy.note)] : []));
    warnSlot.replaceChildren(
      ...(m.buy.largeWarning
        ? [el("div", { id: "large-buy-warning", role: "alert", class: "warning-box" }, el("b", { class: "tone-limit" }, "Before you buy"), el("p", {}, m.buy.largeWarning))]
        : []),
    );
    buySlot.replaceChildren(turnedOffButton("trade-buy-button", m.buy.button.label, m.buy.button.reason));
  };

  listen(buyForm, "submit", (event) => {
    event.preventDefault();
    void (async () => {
      const accountId = account.value.trim();
      if (accountId === "") {
        status.textContent = "Type an account name, for example you.testnet.";
        return;
      }
      const nearIn = parseTokenAmount(amount.value, NEAR_DECIMALS);
      if (nearIn === null) {
        status.textContent = "Type the amount of NEAR as a number, for example 1.5.";
        return;
      }
      status.textContent = "Reading the chain.";
      try {
        const quote = await deps.loadQuote(accountId, nearIn);
        show({ kind: "quote", accountId, nearIn, quote });
        status.textContent = quote.kind === "value" ? `Preview for ${accountId}.` : "No preview from this coin.";
      } catch (e) {
        show({ kind: "none" });
        status.textContent = `Values cannot load from the chain. ${e instanceof Error ? e.message : String(e)}`;
      }
    })();
  });

  const buyPanel = el(
    "div",
    { id: "trade-buy-panel", role: "tabpanel", "aria-labelledby": "trade-tab-buy", class: "trade-panel" },
    buyForm,
    status,
    lines,
    noteSlot,
    warnSlot,
    buySlot,
  );
  const sellPanel = el(
    "div",
    { id: "trade-sell-panel", role: "tabpanel", "aria-labelledby": "trade-tab-sell", class: "trade-panel" },
    el("p", { class: "muted" }, first.sell.pointer),
    el("a", { class: "text-link", href: "#your-plan" }, "Go to Your plan"),
    turnedOffButton("trade-sell-button", first.sell.button.label, first.sell.button.reason),
  );
  sellPanel.hidden = true;

  const pick = (which: "buy" | "sell"): void => {
    tabBuy.setAttribute("aria-selected", String(which === "buy"));
    tabSell.setAttribute("aria-selected", String(which === "sell"));
    buyPanel.hidden = which !== "buy";
    sellPanel.hidden = which !== "sell";
  };
  listen(tabBuy, "click", () => pick("buy"));
  listen(tabSell, "click", () => pick("sell"));

  return el(
    "section",
    { id: "trade-box", "aria-label": "Trade", class: "card trade-box" },
    el("h2", {}, "Trade"),
    el("p", { class: "trade-connect-prompt" }, first.connectPrompt),
    turnedOffButton("trade-connect", first.connect.label, first.connect.reason),
    el("div", { role: "tablist", "aria-label": "Buy or sell", class: "tabs" }, tabBuy, tabSell),
    buyPanel,
    sellPanel,
  );
}
