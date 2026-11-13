// What the "Your plan" card and the sell preview show, as plain text and flags. Pure. Every figure is
// formatted from fc_account or fc_quote_sell; nothing is computed with the coin's rules here.
import type { Account, SellQuote } from "./account-views.ts";
import type { Reading } from "./coin-page.ts";
import { NOT_AVAILABLE, type Tone } from "./coin-page-model.ts";
import { clockTime, compactTokens, nearAmount, timeWithHint, type TimeContext } from "./format.ts";

export interface PlanInput {
  accountId: string;
  symbol: string;
  decimals: number;
  totalSupply: bigint | null;
  account: Reading<Account>;
  /** Null when no amount was asked; then there is no quote and no preview. */
  asked: bigint | null;
  quote: Reading<SellQuote> | null;
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

const REASON_SENTENCE = {
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

export function yourPlanView(input: PlanInput, ctx: TimeContext): { plan: PlanView; preview: PreviewView | null } {
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

function planView(input: PlanInput, a: Account, ctx: TimeContext, tokens: (n: bigint) => string): PlanView {
  const symbol = input.symbol;
  if (a.class === "small") {
    return { kind: "small", accountId: input.accountId, balance: `${tokens(a.balance)} ${symbol}`, text: "No limit applies to your bag." };
  }
  const tags: Tag[] = [{ label: a.class === "large" ? "Large" : "Mid-size", tone: a.class === "large" ? "dark" : "plain" }];
  if (a.inCreatorGroup) tags.push({ label: "Creator group", tone: "plain" });
  if (a.markedBalance > 0n) tags.push({ label: "Early supply", tone: "limit" });
  tags.push(a.canSend ? { label: "Can send", tone: "plain" } : { label: "Cannot send", tone: "limit" });

  const share = input.totalSupply !== null && input.totalSupply > 0n ? ` (${shareText(a.balance, input.totalSupply)} of supply)` : "";
  const maxFull = a.maxFullFillNow.kind === "value" ? a.maxFullFillNow.value : null;
  const unmarked = a.unmarked.kind === "value" ? a.unmarked.value : null;
  const notes: { text: string; tone: Tone }[] = [];
  if (a.isLarge) notes.push({ text: "A large account cannot send tokens until its balance is zero.", tone: "plain" });
  if (a.penaltyAppliesAbove.kind === "value" && a.penaltyAppliesAbove.value !== null) {
    notes.push({ text: `Sells above ${tokens(a.penaltyAppliesAbove.value)} ${symbol} in one window pay a penalty.`, tone: "limit" });
  }
  if (a.maxReceive.kind === "unavailable") notes.push({ text: `Receive limit: ${NOT_AVAILABLE}`, tone: "plain" });
  else if (a.maxReceive.value !== null) {
    notes.push({ text: `A send to this account can carry up to ${tokens(a.maxReceive.value)} ${symbol}.`, tone: "plain" });
  }
  return {
    kind: "card",
    accountId: input.accountId,
    tags,
    balance: `Balance ${tokens(a.balance)} ${symbol}${share}`,
    big: maxFull === null ? NOT_AVAILABLE : tokens(maxFull),
    bigLabel: "can sell now",
    bigSub:
      maxFull !== null && unmarked !== null && maxFull < unmarked.allowance
        ? `${tokens(unmarked.allowance)} again after ${timeWithHint(unmarked.windowEndNs, ctx)}`
        : null,
    bigTone: maxFull !== null && maxFull < a.balance ? "limit" : "plain",
    usedPct: unmarked === null || unmarked.allowance === 0n ? 0 : Number((unmarked.used * 100n) / unmarked.allowance),
    usedLabel: unmarked === null ? NOT_AVAILABLE : `${tokens(unmarked.used)} of ${tokens(unmarked.allowance)} used`,
    refill: unmarked === null ? NOT_AVAILABLE : `Refills ${timeWithHint(unmarked.windowEndNs, ctx)}`,
    notes,
  };
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
  const unmarked = a.unmarked.kind === "value" ? a.unmarked.value : null;
  const limited = q.fill !== "full" || q.limitReason !== null;
  const penalty = q.penaltyNear > 0n;
  const lines: PreviewLine[] = [{ label: "Asked", value: amount(asked), tone: "plain" }];
  const supplyShare = input.totalSupply !== null && input.totalSupply > 0n ? ` (${shareText(q.tokensFilled, input.totalSupply)})` : "";
  if (q.fill !== "full") lines.push({ label: "Filled now", value: `${amount(q.tokensFilled)}${supplyShare}`, tone: "plain" });
  if (q.tokensFilled < asked) lines.push({ label: "Stays in wallet", value: amount(asked - q.tokensFilled), tone: "plain" });
  lines.push({ label: "NEAR out", value: `${nearAmount(q.nearOut.toString())} NEAR`, tone: "plain" });
  if (penalty) lines.push({ label: "Penalty", value: `${nearAmount(q.penaltyNear.toString())} NEAR`, tone: "limit" });
  if (limited) {
    lines.push({
      label: "Can sell more after",
      value: unmarked === null ? NOT_AVAILABLE : timeWithHint(unmarked.windowEndNs, ctx),
      tone: "limit",
    });
  }
  return {
    kind: "preview",
    title: titleOf(q),
    tone: limited || penalty ? "limit" : "plain",
    lines,
    copy: limited ? limitedCopy(q, asked, unmarked, ctx, amount, tokens) : penalty ? penaltyCopy(a, amount) : null,
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
  unmarked: { allowance: bigint; windowEndNs: bigint } | null,
  ctx: TimeContext,
  amount: (n: bigint) => string,
  tokens: (n: bigint) => string,
): string {
  const parts: string[] = [];
  parts.push(q.tokensFilled === 0n ? "Nothing fills now." : `Fills ${tokens(q.tokensFilled)} of ${amount(asked)} now.`);
  if (q.limitReason !== null) parts.push(REASON_SENTENCE[q.limitReason]);
  if (q.tokensFilled < asked) parts.push(`${amount(asked - q.tokensFilled)} stays in your wallet.`);
  parts.push(
    unmarked === null
      ? `How much more can follow, and when: ${NOT_AVAILABLE}.`
      : `You can sell up to ${amount(unmarked.allowance)} more after ${clockTime(unmarked.windowEndNs, ctx)}.`,
  );
  return parts.join(" ");
}

function penaltyCopy(a: Account, amount: (n: bigint) => string): string {
  return a.penaltyAppliesAbove.kind === "value" && a.penaltyAppliesAbove.value !== null
    ? `Sells above ${amount(a.penaltyAppliesAbove.value)} pay a penalty. The penalty stays in the pool.`
    : "Part of this sell pays a penalty. The penalty stays in the pool.";
}
