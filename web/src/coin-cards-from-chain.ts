// Coin list cards read from the chain: the factory's `fc_launch` events name the coins, and each
// coin's views and events fill its card. When any read fails the whole list fails, so the page
// never mixes numbers from different reads.
import { eventsFromLaunch, ViewNotAvailable, type ChainReader, type CoinEvent } from "./chain.ts";
import type { CoinCard } from "./coin-list-model.ts";
import { parseCap, parseEarlyWindow, parseMetadata, parsePool, parseRules, parseTotalSupply } from "./coin-views.ts";
import { age, clockTime, compactTokens, nearAmount, percentOfBps, type TimeContext } from "./format.ts";
import { changeText, linePoints, priceSeries } from "./price-series.ts";

/** The list reads this many of the newest coins, so one page load makes a bounded number of reads. */
export const MOST_COINS_SHOWN = 30;
export const NOT_AVAILABLE = "Not available from this coin";

const HOUR_NS = 3_600_000_000_000n;

async function optional<T>(read: () => Promise<unknown>, parse: (v: unknown) => T): Promise<T | null> {
  try {
    return parse(await read());
  } catch (e) {
    if (e instanceof ViewNotAvailable) return null;
    throw e;
  }
}

function big(v: unknown): bigint {
  return typeof v === "string" && /^\d+$/.test(v) ? BigInt(v) : 0n;
}

function latestReceipt(events: CoinEvent[], symbol: string, launchBlock: number, ctx: TimeContext): { account: string; text: string; sellAt: bigint | null } {
  let sellAt: bigint | null = null;
  let latest: CoinEvent | null = null;
  for (const e of events) {
    const isSell = e.name === "fc_sell";
    const isEarlyBuy = e.name === "fc_buy" && big(e.data.marked_tokens) > 0n;
    if (!isSell && !isEarlyBuy) continue;
    if (latest === null || e.block >= latest.block) latest = e;
    if (isSell && e.timeNs !== null && (sellAt === null || BigInt(e.timeNs) > sellAt)) sellAt = BigInt(e.timeNs);
  }
  if (latest === null) return { account: "", text: "No sells yet.", sellAt };
  const d = latest.data;
  const account = String(d.account ?? "");
  if (latest.name === "fc_buy") {
    return { account, text: `bought ${compactTokens(String(d.tokens_out), 18)} ${symbol} in block ${Math.max(0, latest.block - launchBlock)}. These tokens count as early supply.`, sellAt };
  }
  const asked = big(d.tokens_asked);
  const sold = big(d.tokens_sold);
  if (d.fill === "full" && !d.limit_reason) {
    return { account, text: `sold ${compactTokens(String(sold), 18)} ${symbol}. It filled in full.`, sellAt };
  }
  const until = clockTime(big(d.window_end_ns), ctx);
  const left = big(d.allowance_left);
  const tail = left > 0n ? `It can sell up to ${compactTokens(String(left), 18)} more after ${until}.` : `Its allowance is used up until ${until}.`;
  return { account, text: `tried to sell ${compactTokens(String(asked), 18)} ${symbol}. The rule filled ${compactTokens(String(sold), 18)}. ${tail}`, sellAt };
}

export async function loadCoinCard(reader: ChainReader, coin: string, ctx: TimeContext): Promise<CoinCard> {
  const rules = parseRules(await reader.view("fc_rules"));
  const [pool, early, cap, supply, meta, scan] = await Promise.all([
    optional(() => reader.view("fc_pool"), parsePool),
    optional(() => reader.view("fc_early_window"), parseEarlyWindow),
    optional(() => reader.view("fc_cap"), parseCap),
    optional(() => reader.view("ft_total_supply"), parseTotalSupply),
    optional(() => reader.view("ft_metadata"), parseMetadata),
    eventsFromLaunch(reader, rules.launchBlock),
  ]);
  const symbol = (meta?.symbol || undefined) ?? coin.split(".")[0]!.toUpperCase();
  const ageNs = ctx.nowNs - rules.launchNs;
  const inEarly = early !== null && early.blocksLeft > 0;

  let marketCap = NOT_AVAILABLE;
  let marketCapNear: number | null = null;
  if (pool !== null && supply !== null && pool.tokenReserve > 0n) {
    const yocto = ((pool.nearReserve * supply) / pool.tokenReserve).toString();
    marketCap = `${nearAmount(yocto, 2)} NEAR`;
    marketCapNear = Number(nearAmount(yocto, 2));
  }
  const earlyShare = early !== null && supply !== null && supply > 0n ? percentOfBps(Number((early.earlySupplyTotal * 10000n) / supply)) : NOT_AVAILABLE;

  let capState: CoinCard["cap"] = { label: NOT_AVAILABLE, tone: "open" };
  if (inEarly) capState = { label: `Early window, ${early.blocksLeft} blocks left`, tone: "early" };
  else if (cap !== null && cap.isFull) capState = { label: `Cap full until ${clockTime(cap.windowEndNs, ctx)}`, tone: "full" };
  else if (cap !== null) capState = { label: `Cap open, ${nearAmount(cap.capUsed.toString(), 2)} of ${nearAmount(cap.capTotal.toString(), 2)} NEAR out`, tone: "open" };

  const latest = latestReceipt(scan.events, symbol, rules.launchBlock, ctx);
  const held = scan.events.some(
    (e) => e.name === "fc_sell" && e.data.fill !== "full" && e.timeNs !== null && ctx.nowNs - BigInt(e.timeNs) <= HOUR_NS,
  );
  const prices = priceSeries(scan.events);
  return {
    account: coin,
    name: meta?.name || coin,
    ticker: symbol,
    age: age(ageNs),
    ageSeconds: Number(ageNs / 1_000_000_000n),
    marketCapNear,
    marketCap,
    change: changeText(prices),
    early: inEarly && earlyShare !== NOT_AVAILABLE ? `${earlyShare} so far` : earlyShare,
    cap: capState,
    latest: { account: latest.account, text: latest.text },
    bigSellHeld: held,
    inEarlyWindow: inEarly,
    lastSellAtNs: latest.sellAt,
    line: linePoints(prices, 200, 40, 3),
    source: "chain",
  };
}

/** One card per coin the factory launched, newest first, at most MOST_COINS_SHOWN. */
export async function loadCoinCards(
  factory: ChainReader,
  fromBlock: number,
  readerFor: (coin: string) => ChainReader,
  ctx: TimeContext,
): Promise<CoinCard[]> {
  const { events } = await factory.events(fromBlock);
  const launched = new Map<string, number>();
  for (const e of events) {
    if (e.standard !== "fair_chance_factory" || e.name !== "fc_launch" || typeof e.data.coin !== "string") continue;
    launched.set(e.data.coin, e.block);
  }
  const newest = [...launched].sort((a, b) => b[1] - a[1]).slice(0, MOST_COINS_SHOWN);
  return Promise.all(newest.map(([coin]) => loadCoinCard(readerFor(coin), coin, ctx)));
}
