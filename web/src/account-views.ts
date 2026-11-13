// Typed readings of fc_account and fc_quote_sell. A field the coin does not return becomes
// { kind: "unavailable" }, so the page can say so and never invent a value.
import type { Reading } from "./coin-page.ts";
import { fields, reader, UnexpectedView, type Fields } from "./coin-views.ts";

export type AccountClass = "small" | "band" | "large";

export interface Allowance {
  allowance: bigint;
  used: bigint;
  left: bigint;
  floorLeft: bigint;
  windowEndNs: bigint;
}

/** The early-supply allowance of one account (`fc_account.marked`), in the 24 h window. */
export interface MarkedAllowance extends Allowance {
  shareBps: number;
}

export interface Account {
  class: AccountClass;
  balance: bigint;
  markedBalance: bigint;
  unmarkedBalance: bigint;
  isLarge: boolean;
  canSend: boolean;
  inCreatorGroup: boolean;
  /** Null for a large account; a view that has no such field is "unavailable". */
  maxReceive: Reading<bigint | null>;
  unmarked: Reading<Allowance>;
  marked: Reading<MarkedAllowance>;
  maxFullFillNow: Reading<bigint>;
  /** Null for an account that is not in the band. */
  penaltyAppliesAbove: Reading<bigint | null>;
}

const UNAVAILABLE = { kind: "unavailable" } as const;

function has(f: Fields, key: string): boolean {
  return f[key] !== undefined;
}

function allowance(view: string, value: unknown): Allowance {
  const r = reader(view, fields(value, view));
  return {
    allowance: r.amount("allowance"),
    used: r.amount("used"),
    left: r.amount("left"),
    floorLeft: r.amount("floor_left"),
    windowEndNs: r.amount("window_end_ns"),
  };
}

function markedAllowance(value: unknown): MarkedAllowance {
  const view = "fc_account.marked";
  return { ...allowance(view, value), shareBps: reader(view, fields(value, view)).count("share_bps") };
}

function accountClass(text: string): AccountClass {
  if (text === "small" || text === "band" || text === "large") return text;
  throw new UnexpectedView("fc_account.class is not small, band or large");
}

export function parseAccount(value: unknown): Account {
  const f = fields(value, "fc_account");
  const r = reader("fc_account", f);
  const balance = r.amount("balance");
  const markedBalance = has(f, "marked_balance") ? r.amount("marked_balance") : 0n;
  return {
    class: accountClass(r.text("class")),
    balance,
    markedBalance,
    unmarkedBalance: has(f, "unmarked_balance") ? r.amount("unmarked_balance") : balance - markedBalance,
    isLarge: r.flag("is_large"),
    canSend: r.flag("can_send"),
    inCreatorGroup: has(f, "in_creator_group") ? r.flag("in_creator_group") : false,
    maxReceive: !has(f, "max_receive")
      ? UNAVAILABLE
      : { kind: "value", value: f.max_receive === null ? null : r.amount("max_receive") },
    unmarked: has(f, "unmarked") ? { kind: "value", value: allowance("fc_account.unmarked", f.unmarked) } : UNAVAILABLE,
    marked: has(f, "marked") ? { kind: "value", value: markedAllowance(f.marked) } : UNAVAILABLE,
    maxFullFillNow: has(f, "max_full_fill_now") ? { kind: "value", value: r.amount("max_full_fill_now") } : UNAVAILABLE,
    penaltyAppliesAbove: !has(f, "penalty_applies_above")
      ? UNAVAILABLE
      : { kind: "value", value: f.penalty_applies_above === null ? null : r.amount("penalty_applies_above") },
  };
}

export type SellFill = "full" | "partial" | "none";
export type LimitReason = "allowance" | "cap_full" | "floor";

export interface SellQuote {
  tokensFilled: bigint;
  nearOut: bigint;
  penaltyNear: bigint;
  fill: SellFill;
  limitReason: LimitReason | null;
}

export function parseSellQuote(value: unknown): SellQuote {
  const r = reader("fc_quote_sell", fields(value, "fc_quote_sell"));
  const fill = r.text("fill");
  if (fill !== "full" && fill !== "partial" && fill !== "none") throw new UnexpectedView("fc_quote_sell.fill is not full, partial or none");
  const reason = r.optionalText("limit_reason");
  if (reason !== null && reason !== "allowance" && reason !== "cap_full" && reason !== "floor") {
    throw new UnexpectedView("fc_quote_sell.limit_reason is not allowance, cap_full or floor");
  }
  return {
    tokensFilled: r.amount("tokens_filled"),
    nearOut: r.amount("near_out"),
    penaltyNear: r.amount("penalty_near"),
    fill,
    limitReason: reason,
  };
}
