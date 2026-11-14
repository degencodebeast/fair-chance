// What the "Your plan" card and the sell preview show, as plain text and flags. Pure. Every figure is
// formatted from fc_account or fc_quote_sell; nothing is computed with the coin's rules here.
import type { Account, Allowance, SellQuote } from "./account-views.ts";
import type { Reading } from "./coin-page.ts";
import { NOT_AVAILABLE, type Tone } from "./coin-page-model.ts";
import { clockTime, compactTokens, compactTokensRoundedUp, nearAmount, timeWithHint, type TimeContext } from "./format.ts";

export interface PlanInput {
  accountId: string;
  symbol: string;
  decimals: number;
  totalSupply: bigint | null;
  account: Reading<Account>;
  /** Null when no amount was asked; then there is no quote and no preview. */
  asked: bigint | null;
  quote: Reading<SellQuote> | null;
  /** The quote of N, the early supply a sell takes before the other tokens (see loadAccountPlan). */
  earlySupplyQuote?: { amount: bigint; quote: Reading<SellQuote> } | null;
}

export interface Tag {
  label: string;
  tone: "dark" | "limit" | "plain";
}

export type PlanView =
  | { kind: "unavailable"; accountId: string; text: string }
  | { kind: "small"; accountId: string; balance: string; text: string }
  | {
      kind: "card";
      accountId: string;
      tags: Tag[];
      balance: string;
      big: string;
      bigLabel: string;
      bigSub: string | null;
      bigTone: Tone;
      usedPct: number;
      usedLabel: string;
      refill: string;
      /** The early-supply allowance and reset, when the account also holds other tokens. */
      earlyLine: string | null;
      notes: { text: string; tone: Tone }[];
    };

export interface PreviewLine {
  label: string;
  value: string;
  tone: Tone;
}

export type PreviewView =
  | { kind: "unavailable"; text: string }
  | {
      kind: "preview";
      title: string;
      tone: Tone;
      lines: PreviewLine[];
      copy: string | null;
      bar: { fillPct: number; fills: string; stays: string } | null;
    };

export const REASON_SENTENCE = {
  allowance: "This is all your allowance lets through.",
  cap_full: "The pool cap is full for this hour.",
  floor: "The pool cap is full for this hour, so just a small floor fills.",
} as const;

function shareText(amount: bigint, supply: bigint): string {
  const hundredths = (amount * 10_000n) / supply;
  if (hundredths === 0n) return amount === 0n ? "0%" : "under 0.01%";
  if (hundredths >= 100n) return `${hundredths / 100n}.${(hundredths % 100n) / 10n}%`;
  return `0.${hundredths.toString().padStart(2, "0")}%`;
}

export function yourPlanView(
  input: PlanInput,
  ctx: TimeContext,
): { plan: PlanView; preview: PreviewView | null; asOf: string } {
  return { ...planAndPreview(input, ctx), asOf: `As of ${clockTime(ctx.nowNs, ctx)}` };
}

function planAndPreview(input: PlanInput, ctx: TimeContext): { plan: PlanView; preview: PreviewView | null } {
  const tokens = (n: bigint) => compactTokens(n.toString(), input.decimals);
  if (input.account.kind !== "value") {
    return {
      plan: { kind: "unavailable", accountId: input.accountId, text: NOT_AVAILABLE },
      preview: input.quote === null || input.asked === null ? null : { kind: "unavailable", text: NOT_AVAILABLE },
    };
  }
  const account = input.account.value;
  return {
    plan: planView(input, account, ctx, tokens),
    preview: input.quote === null || input.asked === null ? null : previewView(input, account, input.quote, input.asked, ctx, tokens),
  };
}

/** The allowance the card's bar and reset line follow: other tokens, or early supply when that is all the account holds. */
function primaryAllowance(a: Account): { kind: "unmarked" | "marked"; value: Allowance } | null {
  const marked = a.marked.kind === "value" ? a.marked.value : null;
  const unmarked = a.unmarked.kind === "value" ? a.unmarked.value : null;
  if (marked !== null && a.markedBalance > 0n && (a.unmarkedBalance === 0n || unmarked === null)) return { kind: "marked", value: marked };
  return unmarked === null ? null : { kind: "unmarked", value: unmarked };
}

const PROFIT_HELD_BACK = "Profit from tokens you bought this hour is held back while the pool cap has too little room for it.";

function planView(input: PlanInput, a: Account, ctx: TimeContext, tokens: (n: bigint) => string): PlanView {
  const symbol = input.symbol;
  // Early supply is limited whatever the class says: marked tokens show through marked_balance.
  if (a.class === "small" && a.markedBalance === 0n) {
    const held = a.maxFullFillNow.kind === "value" && a.maxFullFillNow.value < a.balance;
    const text = held ? `No limit applies to your bag. ${PROFIT_HELD_BACK}` : "No limit applies to your bag.";
    return { kind: "small", accountId: input.accountId, balance: `${tokens(a.balance)} ${symbol}`, text };
  }
  const tags: Tag[] = [];
  if (a.class !== "small") tags.push({ label: a.class === "large" ? "Large" : "Mid-size", tone: a.class === "large" ? "dark" : "plain" });
  if (a.inCreatorGroup) tags.push({ label: "Creator group", tone: "plain" });
  if (a.markedBalance > 0n) tags.push({ label: "Early supply", tone: "limit" });
  tags.push(a.canSend ? { label: "Can send", tone: "plain" } : { label: "Cannot send", tone: "limit" });

  const share = input.totalSupply !== null && input.totalSupply > 0n ? ` (${shareText(a.balance, input.totalSupply)} of supply)` : "";
  const maxFull = a.maxFullFillNow.kind === "value" ? a.maxFullFillNow.value : null;
  const primary = primaryAllowance(a);
  const marked = a.marked.kind === "value" ? a.marked.value : null;
  const notes: { text: string; tone: Tone }[] = [];
  if (a.isLarge) notes.push({ text: "A large account cannot send tokens until its balance is zero.", tone: "plain" });
  if (a.penaltyAppliesAbove.kind === "value" && a.penaltyAppliesAbove.value !== null) {
    notes.push({ text: `Sells above ${tokens(a.penaltyAppliesAbove.value)} ${symbol} in one window pay a penalty.`, tone: "limit" });
  }
  if (a.maxReceive.kind === "unavailable") notes.push({ text: `Receive limit: ${NOT_AVAILABLE}`, tone: "plain" });
  else if (a.maxReceive.value !== null) {
    notes.push({ text: `A send to this account can carry up to ${tokens(a.maxReceive.value)} ${symbol}.`, tone: "plain" });
  }
  const usedUnit = primary?.kind === "marked" ? " early supply used" : " used";
  const earlyLine =
    primary?.kind === "unmarked" && marked !== null && a.markedBalance > 0n
      ? `Early supply: ${tokens(marked.left)} of ${tokens(marked.allowance)} left. It resets ${timeWithHint(marked.windowEndNs, ctx)}.`
      : null;
  return {
    kind: "card",
    accountId: input.accountId,
    tags,
    balance: `Balance ${tokens(a.balance)} ${symbol}${share}`,
    big: maxFull === null ? NOT_AVAILABLE : tokens(maxFull),
    bigLabel: "can sell now",
    bigSub:
      maxFull !== null && primary !== null && maxFull < primary.value.allowance
        ? `Allowance this window: ${tokens(primary.value.allowance)}. It resets ${timeWithHint(primary.value.windowEndNs, ctx)}.`
        : null,
    bigTone: maxFull !== null && maxFull < a.balance ? "limit" : "plain",
    usedPct: primary === null || primary.value.allowance === 0n ? 0 : Number((primary.value.used * 100n) / primary.value.allowance),
    usedLabel: primary === null ? NOT_AVAILABLE : `${tokens(primary.value.used)} of ${tokens(primary.value.allowance)}${usedUnit}`,
    refill: primary === null ? NOT_AVAILABLE : `Window resets ${timeWithHint(primary.value.windowEndNs, ctx)}`,
    earlyLine,
    notes,
  };
}

/** The window ends from the coin's views after which this account can sell more; null when the coin gives none. */
function nextSellTimes(a: Account): { label: string; ns: bigint | null }[] {
  const unmarked = a.unmarked.kind === "value" ? a.unmarked.value : null;
  const marked = a.marked.kind === "value" ? a.marked.value : null;
  const early = marked !== null && a.markedBalance > 0n;
  const other = unmarked !== null && (a.unmarkedBalance > 0n || !early);
  const label = "can sell more after";
  if (early && other) {
    return [
      { label: `Early supply ${label}`, ns: marked.windowEndNs },
      { label: `Other tokens ${label}`, ns: unmarked.windowEndNs },
    ];
  }
  if (early) return [{ label: "Can sell more after", ns: marked.windowEndNs }];
  if (other) return [{ label: "Can sell more after", ns: unmarked.windowEndNs }];
  return [{ label: "Can sell more after", ns: null }];
}

function previewView(
  input: PlanInput,
  a: Account,
  quote: Reading<SellQuote>,
  asked: bigint,
  ctx: TimeContext,
  tokens: (n: bigint) => string,
): PreviewView {
  if (quote.kind !== "value") return { kind: "unavailable", text: NOT_AVAILABLE };
  const q = quote.value;
  const symbol = input.symbol;
  const amount = (n: bigint) => `${tokens(n)} ${symbol}`;
  const limited = q.fill !== "full" || q.limitReason !== null;
  const penalty = q.penaltyNear > 0n;
  const lines: PreviewLine[] = [{ label: "Asked", value: amount(asked), tone: "plain" }];
  const supplyShare = input.totalSupply !== null && input.totalSupply > 0n ? ` (${shareText(q.tokensFilled, input.totalSupply)})` : "";
  if (q.fill !== "full") lines.push({ label: "Filled now", value: `${amount(q.tokensFilled)}${supplyShare}`, tone: "plain" });
  if (q.tokensFilled < asked) lines.push({ label: "Stays in wallet", value: amount(asked - q.tokensFilled), tone: "plain" });
  lines.push({ label: "NEAR out", value: `${nearAmount(q.nearOut.toString())} NEAR`, tone: "plain" });
  if (penalty) lines.push({ label: "Penalty", value: `${nearAmount(q.penaltyNear.toString())} NEAR`, tone: "limit" });
  if (limited) {
    for (const t of nextSellTimes(a)) {
      lines.push({ label: t.label, value: t.ns === null ? NOT_AVAILABLE : timeWithHint(t.ns, ctx), tone: "limit" });
    }
  }
  return {
    kind: "preview",
    title: titleOf(q),
    tone: limited || penalty ? "limit" : "plain",
    lines,
    copy: limited
      ? joinCopy(earlySupplyHeldLine(input, a, q), limitedCopy(q, asked, a, ctx, amount, tokens))
      : penalty
        ? penaltyCopy(a, amount)
        : null,
    bar:
      limited && q.tokensFilled < asked
        ? {
            fillPct: Number((q.tokensFilled * 100_000n) / asked) / 1000,
            fills: `${amount(q.tokensFilled)} fills now`,
            stays: `${amount(asked - q.tokensFilled)} stays in your wallet`,
          }
        : null,
  };
}

/**
 * For an account whose early supply the pool cap holds while it has other tokens: a sell takes the
 * early supply before the other tokens, so a sell of N or less fills nothing. N is checked with a
 * quote of N and shown rounded up, so the text is at or above the coin's value; when the coin
 * gives no N, or when the other tokens cannot fill now (the coin's unmarked floor left is 0), the
 * line leaves N out and points to the quote.
 */
function earlySupplyHeldLine(input: PlanInput, a: Account, q: SellQuote): string | null {
  if (a.markedBalance === 0n || a.unmarkedBalance === 0n) return null;
  const lead = "Early-supply tokens sell before your other tokens. While the pool cap holds them,";
  const check = input.earlySupplyQuote ?? null;
  const nQuote = check !== null && check.quote.kind === "value" ? check.quote.value : null;
  const held = nQuote !== null && nQuote.tokensFilled === 0n && nQuote.limitReason === "cap_full";
  if (nQuote !== null && !held) return null;
  const otherTokensFill = a.unmarked.kind === "value" && a.unmarked.value.floorLeft > 0n;
  if (check !== null && held && otherTokensFill) {
    const shown = `${compactTokensRoundedUp(check.amount.toString(), input.decimals)} ${input.symbol}`;
    return `${lead} a sell of about ${shown} or less fills nothing, and a larger sell can fill part of the amount above that from your other tokens.`;
  }
  const nUnknown = held || a.marked.kind === "unavailable" || check !== null;
  const capStops = q.limitReason === "cap_full" || q.limitReason === "floor";
  return nUnknown && capStops ? `${lead} a small sell can fill nothing. The quote shows what this sell fills.` : null;
}

function joinCopy(first: string | null, rest: string): string {
  return first === null ? rest : `${first} ${rest}`;
}

function titleOf(q: SellQuote): string {
  const none = q.fill === "none";
  if (q.limitReason === "floor") return "Pool cap is full. Your sell fills just a small floor";
  if (q.limitReason === "cap_full") return `Pool cap is full. Your sell ${none ? "does not fill now" : "fills in part"}`;
  if (q.limitReason === "allowance" || q.fill === "partial") return `Your sell ${none ? "does not fill now" : "fills in part"}`;
  if (none) return "Your sell does not fill now";
  return q.penaltyNear > 0n ? "Part of your sell pays a penalty" : "Your sell fills in full";
}

function limitedCopy(
  q: SellQuote,
  asked: bigint,
  a: Account,
  ctx: TimeContext,
  amount: (n: bigint) => string,
  tokens: (n: bigint) => string,
): string {
  const parts: string[] = [];
  parts.push(q.tokensFilled === 0n ? "Nothing fills now." : `Fills ${tokens(q.tokensFilled)} of ${amount(asked)} now.`);
  if (q.limitReason !== null) parts.push(REASON_SENTENCE[q.limitReason]);
  if (q.tokensFilled < asked) parts.push(`${amount(asked - q.tokensFilled)} stays in your wallet.`);
  const times = nextSellTimes(a);
  if (times.some((t) => t.ns === null)) parts.push(`When you can sell again: ${NOT_AVAILABLE}.`);
  else if (times.length === 1) parts.push(`You can sell again after ${clockTime(times[0]!.ns!, ctx)}. The new allowance depends on your balance then.`);
  else {
    parts.push(
      `Early supply can sell again after ${clockTime(times[0]!.ns!, ctx)} and other tokens after ${clockTime(times[1]!.ns!, ctx)}. The new allowance depends on your balance then.`,
    );
  }
  return parts.join(" ");
}

function penaltyCopy(a: Account, amount: (n: bigint) => string): string {
  return a.penaltyAppliesAbove.kind === "value" && a.penaltyAppliesAbove.value !== null
    ? `Sells above ${amount(a.penaltyAppliesAbove.value)} pay a penalty. The penalty stays in the pool.`
    : "Part of this sell pays a penalty. The penalty stays in the pool.";
}
