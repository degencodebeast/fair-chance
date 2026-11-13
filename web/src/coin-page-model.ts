// What the coin page shows, as plain text and flags, from what the chain returned. Pure: the same
// reads and clock always give the same page. The renderer only draws this.
import type { CoinEvent } from "./chain.ts";
import type { CheckResult } from "./chain-checks.ts";
import type { CoinPageData, CoinReads } from "./coin-page.ts";
import { age, clockTime, compactTokens, nearAmount, percentOfBps, timeWithHint, type TimeContext } from "./format.ts";
import { sellRow } from "./receipts.ts";

export const NOT_AVAILABLE = "Not available from this coin";
const TOKEN_DECIMALS = 18;
const SECONDS_PER_BLOCK = 0.6;
const NS_PER_SECOND = 1_000_000_000n;

export type Tone = "plain" | "limit";

export interface Line {
  text: string;
  tone: Tone;
}

export interface Stat {
  label: string;
  value: string;
  sub: string;
  available: boolean;
}

export interface Card {
  id: "early" | "large" | "cap";
  title: string;
  chip: { text: string; tone: Tone } | null;
  lines: Line[];
  /** The big figure and the bar of the cap card. */
  meter: { text: string; usedPct: number; tone: Tone } | null;
}

export type Banner =
  | { kind: "early"; blocksLeft: number; block: number; end: number; buyFee: string; feeEnds: string; text: string }
  | { kind: "ended"; end: number; text: string };

export interface ReceiptItem {
  /** Receipt ID of an `fc_sell`; null for the other kinds, which are not sell receipts. */
  sellReceiptId: string | null;
  eventReceiptId: string;
  block: number;
  time: string | null;
  kind: string;
  /** The sentence of the receipt. */
  sentence: string;
  /** The limit and the next sell time, for a limited sell. */
  detail: string | null;
  fromAlert: boolean;
  tone: Tone;
}

export interface CoinPageModel {
  state: "ready";
  header: { name: string; symbol: string; creator: string; launched: string; description: string | null; icon: string | null };
  banner: Banner | null;
  /** Set when any chain check failed or could not run. */
  warning: { title: string; reasons: string[] } | null;
  checks: CheckResult[];
  stats: Stat[];
  /** The three short lines the phone layout shows under the stats. */
  summaryLines: { label: string; value: string; tone: Tone }[];
  who: Card[];
  receipts: ReceiptItem[];
  receiptsEmpty: string | null;
  rules: { label: string; value: string }[];
  codeHash: string;
  /** The claim about the rules that holds only when every check passed. */
  rulesStatement: string | null;
  gaps: string[];
}

export type CoinPageView =
  | CoinPageModel
  | { state: "error"; title: string; detail: string; lastAttempt: string };

const ICON_PREFIX = "data:image/webp;base64,";

export function coinPageModel(data: CoinPageData, ctx: TimeContext): CoinPageView {
  if (data.kind === "error") {
    return {
      state: "error",
      title: "Values cannot load from the chain.",
      detail: `Nothing on this page is shown as live until they load. ${data.message}`,
      lastAttempt: clockTime(data.attemptedAtNs, ctx),
    };
  }
  const r = data.reads;
  const symbol = r.metadata.kind === "value" ? r.metadata.value.symbol : "tokens";
  const supply = r.totalSupply.kind === "value" ? r.totalSupply.value : null;
  const receipts = receiptItems(r.events, { ...ctx, supply, highlight: data.highlightReceipt });
  const who = whoCanSell(r, ctx, symbol, supply);
  const checksOk = r.checks.every((c) => c.status === "pass");
  return {
    state: "ready",
    header: header(r, ctx),
    banner: banner(r),
    warning: checksOk ? null : warning(r.checks),
    checks: r.checks,
    stats: stats(r, ctx, symbol, supply),
    summaryLines: summaryLines(who),
    who,
    receipts,
    receiptsEmpty: receipts.length === 0 ? EMPTY_RECEIPTS : null,
    rules: rulesRows(r),
    codeHash: r.rules.codeHash,
    rulesStatement: checksOk
      ? "Nobody can change these rules. The page checked the coin account on the chain: it runs the known code by hash and has no access keys."
      : null,
    gaps: gaps(r),
  };
}

const EMPTY_RECEIPTS =
  "No sells yet. Each sell will show here: what the seller asked, what filled, and how much more can follow and when.";

function header(r: CoinReads, ctx: TimeContext): CoinPageModel["header"] {
  const icon = r.metadata.kind === "value" ? r.metadata.value.icon : null;
  return {
    name: r.metadata.kind === "value" ? r.metadata.value.name : r.coin,
    symbol: r.metadata.kind === "value" ? r.metadata.value.symbol : "",
    creator: r.rules.creator,
    launched: timeWithHint(r.rules.launchNs, ctx),
    description: r.rules.description,
    icon: icon !== null && icon.startsWith(ICON_PREFIX) ? icon : null,
  };
}

function banner(r: CoinReads): Banner | null {
  if (r.earlyWindow.kind !== "value") return null;
  const e = r.earlyWindow.value;
  const end = e.earlyBlocks;
  if (e.blocksLeft === 0) {
    return {
      kind: "ended",
      end,
      text: "Early supply sells at one shared pace per 24 h window. The figures are in the card below.",
    };
  }
  const blocksIn = (n: number) => `about ${Math.max(1, Math.round(n * SECONDS_PER_BLOCK))} s`;
  const block = r.pool.kind === "value" ? Math.max(0, r.pool.value.block - r.rules.launchBlock) : end - e.blocksLeft;
  const feeEnd = e.feeEndBlock - r.rules.launchBlock;
  return {
    kind: "early",
    blocksLeft: e.blocksLeft,
    block,
    end,
    buyFee: percentOfBps(e.buyFeeBpsNow),
    feeEnds: `Fee ends at block ${feeEnd}, ${blocksIn(Math.max(0, feeEnd - block))}`,
    text: `Buys in the next ${e.blocksLeft} blocks (${blocksIn(e.blocksLeft)}) count as early supply. Early supply sells at one shared pace for the whole group.`,
  };
}

function warning(checks: CheckResult[]): { title: string; reasons: string[] } {
  const failed = checks.filter((c) => c.status === "fail");
  return {
    title: failed.length > 0
      ? "Warning: this account does not match the Fair Chance launch rules."
      : "Warning: this page could not confirm that this account is a Fair Chance coin.",
    reasons: checks.filter((c) => c.status !== "pass").map((c) => `${c.label}: ${c.detail}`),
  };
}

function stats(r: CoinReads, ctx: TimeContext, symbol: string, supply: bigint | null): Stat[] {
  const unavailable = (label: string): Stat => ({ label, value: NOT_AVAILABLE, sub: "", available: false });
  const ageStat: Stat = {
    label: "Age",
    value: age(ctx.nowNs - r.rules.launchNs),
    sub: `launched ${clockTime(r.rules.launchNs, ctx)}`,
    available: true,
  };
  if (r.pool.kind !== "value" || r.pool.value.tokenReserve === 0n) {
    return [unavailable(`Price per 1M ${symbol}`), unavailable("Market cap"), unavailable("Real NEAR in pool"), ageStat];
  }
  const p = r.pool.value;
  const millionTokens = 10n ** BigInt(TOKEN_DECIMALS) * 1_000_000n;
  const priceYocto = (p.nearReserve * millionTokens) / p.tokenReserve;
  return [
    { label: `Price per 1M ${symbol}`, value: `${nearAmount(priceYocto.toString(), 4)} NEAR`, sub: "", available: true },
    supply === null
      ? unavailable("Market cap")
      : { label: "Market cap", value: `${nearAmount(((p.nearReserve * supply) / p.tokenReserve).toString())} NEAR`, sub: "", available: true },
    {
      label: "Real NEAR in pool",
      value: `${nearAmount(p.realNear.toString())} NEAR`,
      sub: `+ ${nearAmount(p.virtualNear.toString())} virtual`,
      available: true,
    },
    ageStat,
  ];
}

function whoCanSell(r: CoinReads, ctx: TimeContext, symbol: string, supply: bigint | null): Card[] {
  const na: Line = { text: NOT_AVAILABLE, tone: "plain" };
  const line = (text: string, tone: Tone = "plain"): Line => ({ text, tone });

  const early: Card = { id: "early", title: "Creator group and early buyers", chip: null, lines: [], meter: null };
  early.lines.push(
    r.pool.kind === "value"
      ? line(`Creator group: ${percentOfBps(r.pool.value.creatorGroupShareBps)} of supply, limit ${percentOfBps(r.rules.creatorCapBps)}`)
      : na,
  );
  if (r.earlyWindow.kind === "value") {
    const e = r.earlyWindow.value;
    early.lines.push(
      supply !== null && supply > 0n
        ? line(`Early supply: ${percentOfBps(Number((e.earlySupplyTotal * 10_000n) / supply))} of supply`)
        : na,
    );
    if (e.blocksLeft > 0) {
      early.lines.push(line(`Early supply cannot sell until block ${e.earlyBlocks} has passed.`, "limit"));
    }
    early.lines.push(
      line(`Shared allowance per 24 h window: ${compactTokens(e.sharedAllowance.toString(), TOKEN_DECIMALS)} ${symbol}, for all early supply together`),
    );
    early.lines.push(line(`Refills ${timeWithHint(nextWindowEnd(r.rules.launchNs, r.rules.windowEarlyS, ctx.nowNs), ctx)}`));
  } else {
    early.lines.push(na);
  }

  const large: Card = { id: "large", title: "Large holders", chip: null, lines: [], meter: null };
  large.lines.push(
    r.pool.kind === "value"
      ? line(`${r.pool.value.largeHolders} ${r.pool.value.largeHolders === 1 ? "large holder" : "large holders"} now`)
      : na,
  );
  large.lines.push(
    line(`Large from ${percentOfBps(r.rules.largeThresholdBps)} of supply. Large accounts cannot send tokens until their balance is zero.`),
  );
  large.lines.push(line(`Each account has its own allowance every ${hoursText(r.rules.windowUnmarkedS)}.`));

  const cap: Card = { id: "cap", title: "Pool cap this hour", chip: null, lines: [], meter: null };
  if (r.cap.kind === "value") {
    const c = r.cap.value;
    const tone: Tone = c.isFull ? "limit" : "plain";
    cap.chip = { text: c.isFull ? "Full" : "Has room", tone };
    cap.lines.push(line(`Pool cap: ${percentOfBps(r.rules.realCapCBps)} of real NEAR per hour, shared by all limited sellers. Now ${nearAmount(c.capTotal.toString())} NEAR.`));
    cap.meter = {
      text: `${nearAmount(c.capLeft.toString())} NEAR left of ${nearAmount(c.capTotal.toString())}`,
      usedPct: c.capTotal === 0n ? 0 : Number((c.capUsed * 100n) / c.capTotal),
      tone,
    };
    cap.lines.push(line(`Resets ${timeWithHint(c.windowEndNs, ctx)}`, tone));
    if (c.isFull) {
      cap.lines.push(line(`Large and early-supply sells fill just a small floor until ${clockTime(c.windowEndNs, ctx)}.`, "limit"));
    }
  } else {
    cap.lines.push(na);
  }
  return [early, large, cap];
}

function summaryLines(who: Card[]): CoinPageModel["summaryLines"] {
  const [early, large, cap] = who as [Card, Card, Card];
  const first = (c: Card, i = 0) => c.lines[i]?.text ?? NOT_AVAILABLE;
  return [
    { label: "Early supply", value: first(early, 1), tone: "plain" },
    { label: "Large holders", value: first(large), tone: "plain" },
    {
      label: "Pool cap",
      value: cap.meter ? (cap.chip?.text === "Full" ? "Full" : cap.meter.text) : NOT_AVAILABLE,
      tone: cap.meter?.tone ?? "plain",
    },
  ];
}

/** The end of the 24 h window that holds `nowNs`, on the fixed grid from launch. */
function nextWindowEnd(launchNs: bigint, windowS: number, nowNs: bigint): bigint {
  const length = BigInt(windowS) * NS_PER_SECOND;
  const k = nowNs >= launchNs ? (nowNs - launchNs) / length : 0n;
  return launchNs + (k + 1n) * length;
}

function hoursText(seconds: number): string {
  return seconds % 3600 === 0 ? `${seconds / 3600} h` : `${Math.round(seconds / 60)} min`;
}

const LIMIT_REASONS: Record<string, string> = {
  allowance: "the account's allowance for this window is used",
  cap_full: "the pool cap is full for this hour",
  floor: "just the small floor fills",
};

function receiptItems(
  events: CoinEvent[],
  ctx: TimeContext & { supply: bigint | null; highlight: string | null },
): ReceiptItem[] {
  const items: ReceiptItem[] = [];
  for (const e of events) {
    if (e.standard !== "fair_chance") continue;
    const d = e.data as Record<string, string | null>;
    const time = e.timeNs === null ? null : timeWithHint(BigInt(e.timeNs), ctx);
    const base = { eventReceiptId: e.receiptId, block: e.block, time, fromAlert: ctx.highlight === e.receiptId };
    if (e.name === "fc_sell") {
      const row = sellRow(e);
      const reason = row.limit_reason === null ? null : (LIMIT_REASONS[row.limit_reason] ?? row.limit_reason);
      const next = d.window_end_ns && /^\d+$/.test(d.window_end_ns) ? ` It can sell more after ${timeWithHint(BigInt(d.window_end_ns), ctx)}.` : "";
      items.push({
        ...base,
        sellReceiptId: row.receipt_id,
        kind: row.fill === "full" ? "Sell" : row.fill === "partial" ? "Partial fill" : "No fill",
        sentence: row.text,
        detail: reason === null ? null : `Limit: ${reason}.${next}`,
        tone: reason === null ? "plain" : "limit",
      });
    } else if (e.name === "fc_cap_filled") {
      items.push({
        ...base,
        sellReceiptId: null,
        kind: "Cap full",
        sentence: `Pool cap full for this hour: ${nearAmount(String(d.cap_used))} NEAR out. Large and early-supply sells fill just a small floor until ${d.window_end_ns ? clockTime(BigInt(d.window_end_ns), ctx) : "the cap resets"}.`,
        detail: null,
        tone: "limit",
      });
    } else if (e.name === "fc_became_large") {
      const share = ctx.supply && ctx.supply > 0n ? ` (${percentOfBps(Number((BigInt(String(d.balance)) * 10_000n) / ctx.supply))} of supply)` : "";
      items.push({
        ...base,
        sellReceiptId: null,
        kind: "Became large",
        sentence: `${d.account} now holds ${compactTokens(String(d.balance), TOKEN_DECIMALS)} tokens${share}. It is large and cannot send tokens.`,
        detail: null,
        tone: "plain",
      });
    }
  }
  // Newest at the top: later block first; inside a block, later event first.
  return items.map((item, index) => ({ item, index })).sort((a, b) => b.item.block - a.item.block || b.index - a.index).map((x) => x.item);
}

function rulesRows(r: CoinReads): { label: string; value: string }[] {
  const x = r.rules;
  const rows = [
    ["Early window", `${x.earlyBlocks} blocks. Early supply cannot sell in it.`],
    ["Allowance window", `${hoursText(x.windowUnmarkedS)}, fixed grid from launch`],
    ["Early-supply allowance", `one shared allowance per ${hoursText(x.windowEarlyS)} window, split by balance`],
    ["Pool cap per hour", `${percentOfBps(x.realCapCBps)} of real NEAR`],
    ["Creator group holds at most", `${percentOfBps(x.creatorCapBps)} of supply`],
    ["Large from", `${percentOfBps(x.largeThresholdBps)} of supply`],
    ["Large accounts can send", "no, until balance is zero"],
    ["Floor fill when the cap is full", percentOfBps(x.floorBps)],
    ["Penalty for a sell above a band allowance", percentOfBps(x.penaltyBps)],
    ["Swap fee", `${percentOfBps(x.swapFeeBps)} on buys and sells`],
    ["Creator share of the swap fee", percentOfBps(x.creatorFeeShareBps)],
    [
      "Creator can claim",
      `after the pool holds at least ${nearAmount(x.claimMinRealNear.toString())} real NEAR for ${hoursText(x.claimDepthHoldS)}`,
    ],
    [
      "Unclaimed creator fees",
      x.unclaimedToPoolS === null ? "stay outside the pool" : `go to the pool after ${hoursText(x.unclaimedToPoolS)}`,
    ],
    ["Virtual NEAR at launch", `${nearAmount(x.virtualNear.toString())} NEAR`],
    ["Graduation", "none, liquidity stays locked"],
    ["Every sell", "needs a wallet confirmation"],
  ];
  return rows.map(([label, value]) => ({ label: label as string, value: value as string }));
}

function gaps(r: CoinReads): string[] {
  const n = r.rules.earlyBlocks;
  return [
    `People who buy after block ${n} through many small wallets.`,
    "Wallets the creator funds but did not create as sub-accounts.",
    "Many mid-size wallets. Each sells one allowance per hour without counting toward the cap.",
    `A buyer in blocks 0 to ${n}, even a small one, sells at the shared early-supply pace.`,
    "A holder who gives away a whole account, keys included.",
    "Two allowances back to back at the end and start of an hour.",
    "Selling by small holders. The rules slow big exits. They do not make the price go up.",
  ];
}
