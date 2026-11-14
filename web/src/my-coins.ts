// The coins of one account: the ones it launched (the factory's fc_launch events with it as the
// creator) and the ones it holds (ft_balance_of above zero), each with what it can sell now from
// fc_quote_sell for its whole balance. Read-only. When any read fails the whole list fails.
import { parseSellQuote } from "./account-views.ts";
import { chainReader, type ChainReader } from "./chain.ts";
import { MOST_COINS_SHOWN } from "./coin-cards-from-chain.ts";
import { parseMetadata } from "./coin-views.ts";
import { resolveListLink } from "./page-link.ts";
import { backoffDelayMs, shouldBackOff } from "./refresher.ts";

/** Coins read at the same time, each one view at a time: at most this many RPC calls in flight. */
const COINS_AT_ONCE = 4;
/** Reads of one coin after a 429 or a 5xx, before the list fails. */
const TRIES = 3;

export interface MyCoinsOptions {
  /** Tests give their own wait. */
  sleep?: (ms: number) => Promise<void>;
}

const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** Runs `read` for each item, at most `limit` at a time, and keeps the order. */
async function inBatches<T, R>(items: T[], limit: number, read: (item: T) => Promise<R>): Promise<R[]> {
  const out: R[] = new Array(items.length);
  let next = 0;
  const worker = async (): Promise<void> => {
    while (next < items.length) {
      const i = next++;
      out[i] = await read(items[i]!);
    }
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
  return out;
}

/** A read that the RPC answered with 429 or a 5xx runs again after the shared backoff. */
async function withBackoff<R>(read: () => Promise<R>, sleep: (ms: number) => Promise<void>): Promise<R> {
  for (let failures = 1; ; failures++) {
    try {
      return await read();
    } catch (e) {
      if (!shouldBackOff(e) || failures >= TRIES) throw e;
      await sleep(backoffDelayMs(failures, e.retryAfterMs));
    }
  }
}

export interface MyCoin {
  coin: string;
  name: string;
  ticker: string;
  icon: string | null;
  decimals: number;
  launchBlock: number;
  balance: bigint;
  /** Tokens of the balance that fill now; 0 when the account holds none. */
  canSellNow: bigint;
}

export interface MyCoins {
  launched: MyCoin[];
  /** Coins with a balance above zero, the launched ones included. */
  held: MyCoin[];
}

function amount(value: unknown, view: string): bigint {
  if (typeof value !== "string" || !/^\d+$/.test(value)) throw new Error(`${view} is not an amount`);
  return BigInt(value);
}

async function readCoin(reader: ChainReader, coin: string, launchBlock: number, accountId: string): Promise<MyCoin> {
  // One view at a time, so COINS_AT_ONCE coins make at most that many RPC calls in flight.
  const meta = await reader.view("ft_metadata");
  const balanceView = await reader.view("ft_balance_of", { account_id: accountId });
  const m = parseMetadata(meta);
  const balance = amount(balanceView, "ft_balance_of");
  const canSellNow = balance === 0n ? 0n : parseSellQuote(await reader.view("fc_quote_sell", { account_id: accountId, amount: balance.toString() })).tokensFilled;
  return { coin, name: m.name, ticker: m.symbol, icon: m.icon, decimals: m.decimals, launchBlock, balance, canSellNow };
}

/** Newest first, over at most MOST_COINS_SHOWN of the factory's coins. */
export async function loadMyCoins(factory: ChainReader, fromBlock: number, readerFor: (coin: string) => ChainReader, accountId: string, options: MyCoinsOptions = {}): Promise<MyCoins> {
  const sleep = options.sleep ?? wait;
  const { events } = await withBackoff(() => factory.events(fromBlock), sleep);
  const launches = new Map<string, { block: number; creator: string }>();
  for (const e of events) {
    if (e.standard !== "fair_chance_factory" || e.name !== "fc_launch" || typeof e.data.coin !== "string") continue;
    launches.set(e.data.coin, { block: e.block, creator: String(e.data.creator ?? "") });
  }
  const newest = [...launches].sort((a, b) => b[1].block - a[1].block).slice(0, MOST_COINS_SHOWN);
  const coins = await inBatches(newest, COINS_AT_ONCE, ([coin, l]) => withBackoff(() => readCoin(readerFor(coin), coin, l.block, accountId), sleep));
  return {
    launched: coins.filter((c) => launches.get(c.coin)!.creator === accountId),
    held: coins.filter((c) => c.balance > 0n),
  };
}

/** The reader of My coins for a page link: the same factory and RPC as the coin list. */
export function myCoinsFromLink(href: string): (accountId: string) => Promise<MyCoins> {
  return async (accountId) => {
    const link = resolveListLink(href);
    if (link.kind !== "chain") throw new Error("This link names no factory to read.");
    const fastNear = link.fastNearUrl ? { fastNearUrl: link.fastNearUrl } : {};
    const factory = chainReader({ rpcUrl: link.rpcUrl, coin: link.factory, anyLoggedTx: true, ...fastNear });
    return loadMyCoins(factory, link.fromBlock, (coin) => chainReader({ rpcUrl: link.rpcUrl, coin, ...fastNear }), accountId);
  };
}
