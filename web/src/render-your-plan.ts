// Draws the "Your plan" section: a form for an account name and an amount, then the plan card and
// the sell preview. There is no sell button here: the page does not send transactions yet.
import { chainReader, configFromUrl } from "./chain.ts";
import type { CoinReads } from "./coin-page.ts";
import { el } from "./dom-elements.ts";
import { loadAccountPlan } from "./your-plan.ts";
import { yourPlanView, type PlanView, type PreviewView } from "./your-plan-model.ts";
import { parseTokenAmount } from "./token-amount.ts";

const DEFAULT_DECIMALS = 18;

function drawPlan(p: PlanView): HTMLElement {
  if (p.kind === "unavailable") {
    return el("div", { class: "plan-card" }, el("h3", { class: "plan-title" }, "Your plan"), el("p", { class: "note" }, p.text));
  }
  if (p.kind === "small") {
    return el(
      "div",
      { class: "plan-quiet", id: "plan-card" },
      el("span", {}, p.text),
      el("span", { class: "num muted" }, `${p.accountId}, ${p.balance}`),
    );
  }
  return el(
    "div",
    { class: "plan-card", id: "plan-card" },
    el("div", { class: "plan-head" }, el("b", {}, "Your plan"), el("span", { class: "num muted" }, p.accountId)),
    el("div", { class: "chips" }, ...p.tags.map((t) => el("span", { class: `chip chip-tag-${t.tone}` }, t.label))),
    el("p", { class: "num muted plan-balance" }, p.balance),
    el("div", { class: "plan-big" }, el("span", { class: `num-big tone-${p.bigTone}` }, p.big), el("span", {}, p.bigLabel)),
    p.bigSub && el("p", { class: "num plan-big-sub" }, p.bigSub),
    el("div", { class: "meter" }, el("div", { class: "meter-used meter-plain", style: `width:${Math.min(100, p.usedPct)}%` })),
    el("div", { class: "plan-used num" }, el("span", {}, p.usedLabel), el("span", {}, p.refill)),
    el("div", { class: "plan-notes" }, ...p.notes.map((n) => el("p", { class: `tone-${n.tone}` }, n.text))),
  );
}

function drawPreview(v: PreviewView): HTMLElement {
  if (v.kind === "unavailable") {
    return el("div", { class: "preview", id: "sell-preview" }, el("b", {}, "Sell preview"), el("p", { class: "note" }, v.text));
  }
  return el(
    "div",
    { class: "preview", id: "sell-preview" },
    el("div", { class: "preview-head" }, el("b", { class: `tone-${v.tone}` }, v.title), el("span", { class: "num muted" }, "preview")),
    v.bar &&
      el(
        "div",
        {},
        el("div", { class: "meter" }, el("div", { class: "meter-used meter-plain", style: `width:${Math.max(1, v.bar.fillPct)}%` })),
        el("div", { class: "preview-bar-labels num" }, el("span", {}, v.bar.fills), el("span", {}, v.bar.stays)),
      ),
    el(
      "dl",
      { class: "preview-lines" },
      ...v.lines.flatMap((l) => [el("dt", {}, l.label), el("dd", { class: `tone-${l.tone}` }, l.value)]),
    ),
    v.copy && el("p", { class: "preview-copy" }, v.copy),
  );
}

export function drawYourPlan(reads: CoinReads): HTMLElement {
  const result = el("div", { id: "plan-result", role: "status", "aria-live": "polite" });
  const accountInput = el("input", { id: "plan-account", name: "account", type: "text", autocomplete: "off", spellcheck: "false", placeholder: "you.testnet" });
  const amountInput = el("input", { id: "plan-amount", name: "amount", type: "text", inputmode: "decimal", autocomplete: "off", placeholder: "30,000,000" });
  const decimals = reads.metadata.kind === "value" ? reads.metadata.value.decimals : DEFAULT_DECIMALS;
  const symbol = reads.metadata.kind === "value" ? reads.metadata.value.symbol : "tokens";
  const supply = reads.totalSupply.kind === "value" ? reads.totalSupply.value : null;

  const show = async (): Promise<void> => {
    const accountId = accountInput.value.trim();
    if (accountId === "") {
      result.replaceChildren(el("p", { class: "note" }, "Type an account name, for example you.testnet."));
      return;
    }
    const typed = amountInput.value.trim();
    const asked = typed === "" ? null : parseTokenAmount(typed, decimals);
    if (typed !== "" && asked === null) {
      result.replaceChildren(el("p", { class: "note" }, `Type the amount as a number of ${symbol}, for example 1,000,000.`));
      return;
    }
    result.replaceChildren(el("p", { class: "note" }, "Reading the chain."));
    const nowNs = BigInt(Date.now()) * 1_000_000n;
    try {
      const chain = chainReader(configFromUrl(location.href));
      const loaded = await loadAccountPlan(chain, { accountId, asked });
      const view = yourPlanView({ accountId, symbol, decimals, totalSupply: supply, ...loaded }, { nowNs });
      result.replaceChildren(drawPlan(view.plan), ...(view.preview ? [drawPreview(view.preview)] : []));
    } catch (e) {
      const message = e instanceof Error ? e.message : String(e);
      result.replaceChildren(el("div", { role: "alert", class: "banner banner-error" }, el("b", {}, "Values cannot load from the chain."), ` ${message}`));
    }
  };

  const form = el(
    "form",
    { class: "plan-form" },
    el("label", { for: "plan-account" }, "Account name"),
    accountInput,
    el("label", { for: "plan-amount" }, `Amount to sell (${symbol}), optional`),
    amountInput,
    el("button", { type: "submit", class: "button button-dark" }, "Show plan and preview"),
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    void show();
  });
  return el(
    "section",
    { id: "your-plan", "aria-label": "Your plan", class: "card" },
    el("h2", {}, "Your plan"),
    el("p", { class: "muted" }, "Type any account to see what it can sell now. Add an amount to preview a sell. The coin works these out, not this page."),
    form,
    result,
  );
}
