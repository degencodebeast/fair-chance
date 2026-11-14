// The trade box: a buy with its preview, and a sell. The visitor's wallet signs; this file only
// asks for it through the trade actions and shows the result. The buy preview reads fc_quote_buy
// for the account and NEAR amount typed. Text comes from the trade box model and the result text.
import type { BuyQuote } from "./buy-quote.ts";
import type { Reading } from "./coin-page.ts";
import { el } from "./dom-elements.ts";
import { listen } from "./dom-listen.ts";
import { compactTokens } from "./format.ts";
import { COMMA_REFUSAL, formatTokenAmount, hasComma, parseTokenAmount } from "./token-amount.ts";
import type { PreviewedBuy, SellLimit, TradeOutcome } from "./trade-actions.ts";
import { tradeBoxModel, type BuyPreview, type TradeButton } from "./trade-box-model.ts";
import { tradeResultText } from "./trade-result-text.ts";
import type { WalletSession } from "./tx-types.ts";
import type { WalletControl } from "./wallet-session.ts";

const NEAR_DECIMALS = 24;

export interface TradeActions {
  /** Sends exactly the previewed buy: its NEAR amount and its quote. */
  buy(session: WalletSession, buy: PreviewedBuy): Promise<TradeOutcome>;
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
  /** The warning when the coin fails the page's chain checks; null when every check passes. Then nothing is sent. */
  blocked: string | null;
}

/** A button that is on with its handler, or off with its reason beside it. */
function tradeButton(id: string, b: TradeButton, onClick: (() => void) | null): HTMLElement {
  if (b.reason === null) {
    const button = el("button", { type: onClick === null ? "submit" : "button", id, class: "button button-dark button-primary" }, b.label);
    if (onClick !== null) listen(button, "click", onClick);
    return el("span", { class: "on-control" }, button);
  }
  return el(
    "span",
    { class: "off-control" },
    el("button", { type: "button", id, class: "button button-off button-primary", disabled: "", "aria-describedby": `${id}-reason` }, b.label),
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
    tradeBoxModel({ symbol: deps.symbol, decimals: deps.decimals, largeThresholdBps: deps.largeThresholdBps, storageMin: deps.storageMin, preview, signedIn: session()?.accountId ?? null, pending, blocked: deps.blocked });
  const first = model();

  const tabBuy = el("button", { type: "button", role: "tab", id: "trade-tab-buy", "aria-selected": "true", "aria-controls": "trade-buy-panel", class: "trade-tab" }, "Buy");
  const tabSell = el("button", { type: "button", role: "tab", id: "trade-tab-sell", "aria-selected": "false", "aria-controls": "trade-sell-panel", class: "trade-tab" }, "Sell");

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
  // A share of the balance in one click, as on other launchpads. Max is the whole balance: the coin
  // fills what its limits let through now, and the rest stays in the wallet.
  // What the page knows of the balance: one state for each case, so each says its own sentence.
  type Held = { kind: "signed-out" } | { kind: "loading" } | { kind: "failed" } | { kind: "value"; units: bigint };
  let held: Held = { kind: "signed-out" };
  const heldUnits = (): bigint | null => (held.kind === "value" && held.units > 0n ? held.units : null);
  // After a sell with no clear result, Sell stays off until a new balance read.
  let sellLocked = false;
  let sentSellText: string | null = null;
  const sellSlider = el("input", { id: "trade-sell-slider", type: "range", min: "0", max: "100", step: "1", value: "0", "aria-label": "Share of your balance to sell" });
  const setShare = (pct: number): void => {
    const units0 = heldUnits();
    if (units0 === null) {
      sellAmount.value = "";
      sellSlider.value = "0";
      sellStatus.textContent =
        held.kind === "signed-out" ? "Connect a wallet to sell."
        : held.kind === "loading" ? "Reading your balance. Try again in a moment."
        : held.kind === "failed" ? "Your balance cannot load, so the page cannot fill a share."
        : `You hold no ${deps.symbol} yet.`;
      return;
    }
    // A part share rounds down to whole tokens, so the field stays short; Max keeps every base unit.
    const one = 10n ** BigInt(deps.decimals);
    const part = (units0 * BigInt(pct)) / 100n;
    const units = pct >= 100 ? units0 : part >= one ? (part / one) * one : part;
    sellAmount.value = units === 0n ? "" : formatTokenAmount(units, deps.decimals);
    sellSlider.value = String(pct);
    sellStatus.textContent = units === 0n && pct > 0 ? "Your balance is too small for this share. Use Max." : "";
  };
  const shares = el(
    "div",
    { id: "trade-sell-shares", class: "sell-shares", role: "group", "aria-label": "Share of your balance" },
    ...[25, 50, 75, 100].map((pct) => {
      const b = el("button", { type: "button", class: "chip sell-share" }, pct === 100 ? "Max" : `${pct}%`);
      listen(b, "click", () => setShare(pct));
      return b;
    }),
  );
  listen(sellSlider, "input", () => setShare(Number(sellSlider.value)));
  // A typed amount moves the slider to its share of the balance.
  listen(sellAmount, "input", () => {
    const units = parseTokenAmount(sellAmount.value, deps.decimals);
    const h = heldUnits();
    if (h === null || units === null) return;
    sellSlider.value = String(units >= h ? 100 : Number((units * 100n) / h));
  });
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
    if (s === null || pending || deps.blocked !== null) return;
    pending = true;
    result.replaceChildren();
    redraw();
    void run(s)
      .catch((e: unknown): TradeOutcome => ({ kind: "unknown", text: `The page could not finish this: ${e instanceof Error ? e.message : String(e)}` }))
      .then((outcome) => {
        pending = false;
        showResult(outcome);
        redraw();
        const wasSell = sentSellText !== null;
        // A sell that reached the wallet empties its field, so a second click cannot send it again.
        // An amount typed while the sell waited stays.
        if (wasSell && outcome.kind !== "not-sent" && sellAmount.value === sentSellText) {
          sellAmount.value = "";
          sellSlider.value = "0";
        }
        sentSellText = null;
        // With no clear result the sell may be on chain: Sell stays off until the balance is read again.
        if (wasSell && outcome.kind === "unknown") {
          sellLocked = true;
          redraw();
          loadSellLimit();
        }
        if (outcome.kind === "result") {
          deps.actions.onTraded();
          loadSellLimit();
        }
      });
  };

  const onBuy = (): void => {
    const p = preview;
    // Only the buy the button shows: the previewed amount and quote, for the signed-in account.
    if (p.kind !== "quote" || p.quote.kind !== "value" || p.accountId !== session()?.accountId) return;
    // The fields must still say what was previewed; an edit the page did not hear of drops the preview.
    if (account.value.trim() !== p.accountId || parseTokenAmount(amount.value, NEAR_DECIMALS) !== p.nearIn) return dropPreview();
    const buy: PreviewedBuy = { nearIn: p.nearIn, quote: p.quote.value };
    trade((s) => deps.actions.buy(s, buy));
  };

  let latestLimit = 0;
  const loadSellLimit = (): void => {
    const s = session();
    const mine = ++latestLimit;
    if (s === null) {
      held = { kind: "signed-out" };
      sellLimit.textContent = "Connect a wallet to sell.";
      return;
    }
    held = { kind: "loading" };
    sellLimit.textContent = "Reading what you can sell now.";
    deps.actions.sellLimit(s.accountId).then(
      (l) => {
        if (mine !== latestLimit) return;
        held = { kind: "value", units: l.balance };
        if (sellLocked) {
          sellLocked = false;
          redraw();
        }
        const t = (units: bigint) => `${compactTokens(units.toString(), deps.decimals)} ${deps.symbol}`;
        sellLimit.textContent = l.balance === 0n
            ? `You hold no ${deps.symbol} yet.`
            : `You can sell up to ${t(l.canSellNow)} now. You hold ${t(l.balance)}.${l.moreOpens === null ? "" : ` ${l.moreOpens}`}`;
      },
      (e: unknown) => {
        if (mine !== latestLimit) return;
        held = { kind: "failed" };
        sellLimit.textContent = `What you can sell now cannot load. ${e instanceof Error ? e.message : String(e)}`;
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
    sellSlot.replaceChildren(
      tradeButton("trade-sell-button", sellLocked ? { label: m.sell.button.label, reason: "Check your wallet activity. Sell turns on again when your balance is read." } : m.sell.button, null),
    );
    if (session() === null) {
      const connect = el("button", { type: "button", id: "trade-connect", class: "button button-dark button-primary" }, m.connect.label);
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
    el("button", { type: "submit", id: "trade-buy-preview", class: "button button-outline" }, "Preview buy"),
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
      if (hasComma(amount.value)) {
        preview = { kind: "none" };
        redraw();
        status.textContent = COMMA_REFUSAL;
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

  // Any edit of the account or the amount drops the preview, so the buy button never sends an amount or a quote the box does not show.
  const dropPreview = (): void => {
    latest++;
    if (preview.kind === "none") return;
    preview = { kind: "none" };
    redraw();
    status.textContent = "Preview the buy again for the new account or amount.";
  };
  listen(account, "input", dropPreview);
  listen(amount, "input", dropPreview);

  const sellForm = el(
    "form",
    { id: "trade-sell-form", class: "plan-form" },
    el("label", { for: "trade-sell-amount" }, first.sell.amountLabel),
    sellAmount,
    shares,
    sellSlider,
    sellSlot,
  );
  listen(sellForm, "submit", (event) => {
    event.preventDefault();
    if (sellLocked) return;
    if (hasComma(sellAmount.value)) {
      sellStatus.textContent = COMMA_REFUSAL;
      return;
    }
    const tokens = parseTokenAmount(sellAmount.value, deps.decimals);
    if (tokens === null || tokens === 0n) {
      sellStatus.textContent = `Type the amount of ${deps.symbol} as a number, for example 1.5.`;
      return;
    }
    sellStatus.textContent = "";
    if (session() === null || pending || deps.blocked !== null) return;
    sentSellText = sellAmount.value;
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
    el("div", { role: "tablist", "aria-label": "Buy or sell", class: "trade-tabs" }, tabBuy, tabSell),
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
