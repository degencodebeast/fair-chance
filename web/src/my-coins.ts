// The coins of one account: the ones it launched (the factory's fc_launch events with it as the
// creator) and the ones it holds (ft_balance_of above zero), each with what it can sell now from
// fc_quote_sell for its whole balance. Read-only. When any read fails the whole list fails.
import { parseSellQuote } from "./account-views.ts";
import { chainReader, type ChainReader } from "./chain.ts";
import { MOST_COINS_SHOWN } from "./coin-cards-from-chain.ts";
import { parseMetadata } from "./coin-views.ts";
import { resolveListLink } from "./page-link.ts";

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
  const [meta, balanceView] = await Promise.all([reader.view("ft_metadata"), reader.view("ft_balance_of", { account_id: accountId })]);
  const m = parseMetadata(meta);
  const balance = amount(balanceView, "ft_balance_of");
  const canSellNow = balance === 0n ? 0n : parseSellQuote(await reader.view("fc_quote_sell", { account_id: accountId, amount: balance.toString() })).tokensFilled;
  return { coin, name: m.name, ticker: m.symbol, icon: m.icon, decimals: m.decimals, launchBlock, balance, canSellNow };
}

/** Newest first, over at most MOST_COINS_SHOWN of the factory's coins. */
export async function loadMyCoins(factory: ChainReader, fromBlock: number, readerFor: (coin: string) => ChainReader, accountId: string): Promise<MyCoins> {
  const { events } = await factory.events(fromBlock);
  const launches = new Map<string, { block: number; creator: string }>();
  for (const e of events) {
    if (e.standard !== "fair_chance_factory" || e.name !== "fc_launch" || typeof e.data.coin !== "string") continue;
    launches.set(e.data.coin, { block: e.block, creator: String(e.data.creator ?? "") });
  }
  const newest = [...launches].sort((a, b) => b[1].block - a[1].block).slice(0, MOST_COINS_SHOWN);
  const coins = await Promise.all(newest.map(([coin, l]) => readCoin(readerFor(coin), coin, l.block, accountId)));
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
