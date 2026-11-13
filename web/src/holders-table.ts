// The holders table: one row per account the coin's events name, read from the coin's fc_account
// view. The page does no rule math: "can sell now" is `max_full_fill_now` and the reset time is the
// window end the view gives.
import type { ChainReader, CoinEvent } from "./chain.ts";
import type { Account } from "./account-views.ts";
import { parseAccount } from "./account-views.ts";
import { compactTokens, percentOfBps, timeWithHint, type TimeContext } from "./format.ts";

/** The table reads this many of the most recently active accounts. */
export const MOST_HOLDERS_READ = 25;
const NOT_AVAILABLE = "Not available from this coin";

export interface HolderRow {
  account: string;
  balance: string;
  share: string;
  tags: string[];
  now: string;
  send: "Yes" | "No";
  /** The holder is held below a full sell now: drawn in the limit colour. */
  limited: boolean;
}

function shareText(balance: bigint, supply: bigint): string {
  if (supply === 0n || balance === 0n) return "0%";
  const bps = (balance * 10000n) / supply;
  return bps < 10n ? "<0.1%" : percentOfBps(Number(bps));
}

function afterText(ns: bigint, ctx: TimeContext): string {
  return timeWithHint(ns, ctx).split(",")[0]!;
}

export function holderRow(account: string, a: Account, supply: bigint, ctx: TimeContext): HolderRow {
  const tags: string[] = [];
  if (a.class === "large") tags.push("Large");
  else if (a.class === "band") tags.push("Mid-size");
  if (a.inCreatorGroup) tags.push("Creator group");
  else if (a.markedBalance > 0n) tags.push("Early supply");
  if (tags.length === 0) tags.push("Small");

  let now = NOT_AVAILABLE;
  let limited = false;
  if (a.maxFullFillNow.kind === "value") {
    const full = a.maxFullFillNow.value;
    const windowEnd =
      a.unmarkedBalance > 0n && a.unmarked.kind === "value" ? a.unmarked.value.windowEndNs : a.marked.kind === "value" ? a.marked.value.windowEndNs : null;
    if (full >= a.balance) now = "All of it";
    else {
      limited = true;
      const more = windowEnd === null ? "" : `, more after ${afterText(windowEnd, ctx)}`;
      now = full === 0n ? `0${more}` : `${compactTokens(full.toString(), 18)} now${more}`;
    }
  }
  return { account, balance: compactTokens(a.balance.toString(), 18), share: shareText(a.balance, supply), tags, now, send: a.canSend ? "Yes" : "No", limited };
}

function namedAccounts(e: CoinEvent): string[] {
  const d = e.data;
  const names = e.name === "fc_send" ? [d.from, d.to] : [d.account, d.by_account];
  return names.filter((n): n is string => typeof n === "string");
}

export async function loadHolders(
  chain: ChainReader,
  events: CoinEvent[],
  supply: bigint,
  ctx: TimeContext,
): Promise<{ rows: HolderRow[]; notRead: number }> {
  // Newest event first, so the accounts read are the most recently active ones.
  const seen: string[] = [];
  for (const e of [...events].sort((a, b) => b.block - a.block)) {
    for (const name of namedAccounts(e)) if (!seen.includes(name)) seen.push(name);
  }
  const read = seen.slice(0, MOST_HOLDERS_READ);
  const accounts = await Promise.all(read.map(async (id) => [id, parseAccount(await chain.view("fc_account", { account_id: id }))] as const));
  const rows = accounts
    .filter(([, a]) => a.balance > 0n)
    .sort(([ia, a], [ib, b]) => (a.balance === b.balance ? ia.localeCompare(ib) : a.balance < b.balance ? 1 : -1))
    .map(([id, a]) => holderRow(id, a, supply, ctx));
  return { rows, notRead: seen.length - read.length };
}
