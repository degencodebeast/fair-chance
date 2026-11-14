// The trade box: a buy with its preview, and a sell. The visitor's wallet signs; this file only
// asks for it through the trade actions and shows the result. The buy preview reads fc_quote_buy
// for the account and NEAR amount typed. Text comes from the trade box model and the result text.
import type { BuyQuote } from "./buy-quote.ts";
import type { Reading } from "./coin-page.ts";
import { el } from "./dom-elements.ts";
import { listen } from "./dom-listen.ts";
import { compactTokens } from "./format.ts";
import { parseTokenAmount } from "./token-amount.ts";
import type { SellLimit, TradeOutcome } from "./trade-actions.ts";
import { tradeBoxModel, type BuyPreview, type TradeButton } from "./trade-box-model.ts";
import { tradeResultText } from "./trade-result-text.ts";
import type { WalletSession } from "./tx-types.ts";
import type { WalletControl } from "./wallet-session.ts";

const NEAR_DECIMALS = 24;

export interface TradeActions {
  buy(session: WalletSession, nearIn: bigint): Promise<TradeOutcome>;
  sell(session: WalletSession, amount: bigint): Promise<TradeOutcome>;
  sellLimit(accountId: string): Promise<SellLimit>;
  /** Called after a transaction reached the chain, so the page reads the coin again. */
  onTraded(): void;
}

export interface TradeBoxDeps {
  symbol: string;
  decimals: number;
  largeThresholdBps: number;
  storageMin: Reading<bigint>;
  loadQuote: (accountId: string, nearIn: bigint) => Promise<Reading<BuyQuote>>;
  wallet: WalletControl;
  actions: TradeActions;
}

/** A button that is on with its handler, or off with its reason beside it. */
function tradeButton(id: string, b: TradeButton, onClick: (() => void) | null): HTMLElement {
  if (b.reason === null) {
    const button = el("button", { type: onClick === null ? "submit" : "button", id, class: "button button-dark" }, b.label);
    if (onClick !== null) listen(button, "click", onClick);
    return el("span", { class: "on-control" }, button);
  }
  return el(
    "span",
    { class: "off-control" },
    el("button", { type: "button", id, class: "button button-off", disabled: "", "aria-describedby": `${id}-reason` }, b.label),
    el("span", { id: `${id}-reason`, class: "muted" }, b.reason),
  );
}

export function drawTradeBox(deps: TradeBoxDeps): HTMLElement {
  let preview: BuyPreview = { kind: "none" };
  let pending = false;
  const session = (): WalletSession | null => {
    const s = deps.wallet.state();
    return s.kind === "signed-in" ? s.session : null;
  };
  const model = () =>
    tradeBoxModel({ symbol: deps.symbol, decimals: deps.decimals, largeThresholdBps: deps.largeThresholdBps, storageMin: deps.storageMin, preview, signedIn: session()?.accountId ?? null, pending });
  const first = model();

  const tabBuy = el("button", { type: "button", role: "tab", id: "trade-tab-buy", "aria-selected": "true", "aria-controls": "trade-buy-panel", class: "tab" }, "Buy");
  const tabSell = el("button", { type: "button", role: "tab", id: "trade-tab-sell", "aria-selected": "false", "aria-controls": "trade-sell-panel", class: "tab" }, "Sell");

  const connectSlot = el("div", { id: "trade-connect-slot" });
  const account = el("input", { id: "trade-buy-account", name: "account", type: "text", autocomplete: "off", spellcheck: "false", placeholder: "you.testnet" });
  const amount = el("input", { id: "trade-buy-amount", name: "near", type: "text", inputmode: "decimal", autocomplete: "off", placeholder: "1.00" });
  const status = el("p", { id: "trade-buy-status", class: "muted", role: "status" }, "Type an account and an amount to preview a buy.");
  const lines = el("dl", { id: "trade-buy-lines", class: "preview-lines" });
  const noteSlot = el("div", { id: "trade-buy-note" });
  const warnSlot = el("div", { id: "trade-buy-warning-slot" });
  const storage = el("p", { id: "trade-buy-storage", class: "muted" }, first.buy.storageNote);
  const buySlot = el("div", { id: "trade-buy-action" });
  const result = el("div", { id: "trade-result", role: "status", "aria-live": "polite" });

  const sellLimit = el("p", { id: "trade-sell-limit", class: "muted" });
  const sellAmount = el("input", { id: "trade-sell-amount", name: "tokens", type: "text", inputmode: "decimal", autocomplete: "off", placeholder: "1000" });
  const sellStatus = el("p", { id: "trade-sell-status", class: "muted", role: "status" });
  const sellSlot = el("div", { id: "trade-sell-action" });

  const showResult = (outcome: TradeOutcome): void => {
    const t = tradeResultText(outcome, { symbol: deps.symbol, decimals: deps.decimals });
    result.className = `trade-result trade-result-${t.tone}`;
    result.replaceChildren(
      el("p", {}, el("b", {}, t.headline)),
      ...t.lines.map((l) => el("p", {}, l)),
      ...(t.explorerUrl === null ? [] : [el("a", { class: "text-link", href: t.explorerUrl, target: "_blank", rel: "noopener noreferrer" }, "See it on the testnet explorer")]),
    );
  };

  /** Runs one trade: the buttons stay off until the wallet answers. */
  const trade = (run: (s: WalletSession) => Promise<TradeOutcome>): void => {
    const s = session();
    if (s === null || pending) return;
    pending = true;
    result.replaceChildren();
    redraw();
    void run(s)
      .catch((e: unknown): TradeOutcome => ({ kind: "unknown", text: `The page could not finish this: ${e instanceof Error ? e.message : String(e)}` }))
      .then((outcome) => {
        pending = false;
        showResult(outcome);
        redraw();
        if (outcome.kind === "result") {
          deps.actions.onTraded();
          loadSellLimit();
        }
      });
  };

  const onBuy = (): void => {
    if (preview.kind !== "quote") return;
    const nearIn = preview.nearIn;
    trade((s) => deps.actions.buy(s, nearIn));
  };

  let latestLimit = 0;
  const loadSellLimit = (): void => {
    const s = session();
    const mine = ++latestLimit;
    if (s === null) {
      sellLimit.textContent = "";
      return;
    }
    sellLimit.textContent = "Reading what you can sell now.";
    deps.actions.sellLimit(s.accountId).then(
      (l) => {
        if (mine !== latestLimit) return;
        const t = (units: bigint) => `${compactTokens(units.toString(), deps.decimals)} ${deps.symbol}`;
        sellLimit.textContent = l.balance === 0n ? `This account holds no ${deps.symbol}.` : `You can sell up to ${t(l.canSellNow)} now. You hold ${t(l.balance)}.`;
      },
      (e: unknown) => {
        if (mine === latestLimit) sellLimit.textContent = `What you can sell now cannot load. ${e instanceof Error ? e.message : String(e)}`;
      },
    );
  };

  const redraw = (): void => {
    const m = model();
    lines.replaceChildren(...m.buy.lines.flatMap((l) => [el("dt", {}, l.label), el("dd", {}, l.value)]));
    noteSlot.replaceChildren(...(m.buy.note ? [el("p", { class: "note" }, m.buy.note)] : []));
    warnSlot.replaceChildren(
      ...(m.buy.largeWarning
        ? [el("div", { id: "large-buy-warning", role: "alert", class: "warning-box" }, el("b", { class: "tone-limit" }, "Before you buy"), el("p", {}, m.buy.largeWarning))]
        : []),
    );
    buySlot.replaceChildren(tradeButton("trade-buy-button", m.buy.button, onBuy));
    sellSlot.replaceChildren(tradeButton("trade-sell-button", m.sell.button, null));
    if (session() === null) {
      const connect = el("button", { type: "button", id: "trade-connect", class: "button button-dark" }, m.connect.label);
      listen(connect, "click", () => void deps.wallet.connect());
      connectSlot.replaceChildren(el("p", { class: "trade-connect-prompt" }, m.connectPrompt), connect);
    } else {
      connectSlot.replaceChildren();
    }
  };

  let latest = 0;
  const buyForm = el(
    "form",
    { id: "trade-buy-form", class: "plan-form" },
    el("label", { for: "trade-buy-account" }, "Account name"),
    account,
    el("label", { for: "trade-buy-amount" }, first.buy.amountLabel),
    amount,
    el("button", { type: "submit", id: "trade-buy-preview", class: "button" }, "Preview buy"),
  );
  listen(buyForm, "submit", (event) => {
    event.preventDefault();
    const mine = ++latest;
    void (async () => {
      const accountId = account.value.trim();
      if (accountId === "") {
        preview = { kind: "none" };
        redraw();
        status.textContent = "Type an account name, for example you.testnet.";
        return;
      }
      const nearIn = parseTokenAmount(amount.value, NEAR_DECIMALS);
      if (nearIn === null) {
        preview = { kind: "none" };
        redraw();
        status.textContent = "Type the amount of NEAR as a number, for example 1.5.";
        return;
      }
      status.textContent = "Reading the chain.";
      try {
        const quote = await deps.loadQuote(accountId, nearIn);
        if (mine !== latest) return;
        preview = { kind: "quote", accountId, nearIn, quote };
        redraw();
        status.textContent = quote.kind === "value" ? `Preview for ${accountId}.` : "No preview from this coin.";
      } catch (e) {
        if (mine !== latest) return;
        preview = { kind: "none" };
        redraw();
        status.textContent = `Values cannot load from the chain. ${e instanceof Error ? e.message : String(e)}`;
      }
    })();
  });

  const sellForm = el(
    "form",
    { id: "trade-sell-form", class: "plan-form" },
    el("label", { for: "trade-sell-amount" }, first.sell.amountLabel),
    sellAmount,
    sellSlot,
  );
  listen(sellForm, "submit", (event) => {
    event.preventDefault();
    const tokens = parseTokenAmount(sellAmount.value, deps.decimals);
    if (tokens === null || tokens === 0n) {
      sellStatus.textContent = `Type the amount of ${deps.symbol} as a number, for example 1.5.`;
      return;
    }
    sellStatus.textContent = "";
    trade((s) => deps.actions.sell(s, tokens));
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
    storage,
  );
  const sellPanel = el(
    "div",
    { id: "trade-sell-panel", role: "tabpanel", "aria-labelledby": "trade-tab-sell", class: "trade-panel" },
    sellLimit,
    sellForm,
    sellStatus,
    el("p", { class: "muted" }, first.sell.pointer),
    el("a", { class: "text-link", href: "#your-plan" }, "Go to Your plan"),
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

  const root = el(
    "section",
    { id: "trade-box", "aria-label": "Trade", class: "card trade-box" },
    el("h2", {}, "Trade"),
    connectSlot,
    el("div", { role: "tablist", "aria-label": "Buy or sell", class: "tabs" }, tabBuy, tabSell),
    buyPanel,
    sellPanel,
    result,
  );

  /** A new wallet state: fill in the account, redraw the buttons and read what it can sell. */
  const onWallet = (): void => {
    const s = session();
    if (s !== null && account.value.trim() === "") account.value = s.accountId;
    redraw();
    loadSellLimit();
  };
  let shown = false;
  const stop = deps.wallet.subscribe(() => {
    if (root.isConnected) shown = true;
    else if (shown) return stop();
    onWallet();
  });
  onWallet();
  return root;
}
