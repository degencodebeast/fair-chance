// The steps of a buy and a sell from the page: read the coin's quote, build the transaction, ask the
// wallet to sign and send it, and read the result from the final outcome. The page signs nothing:
// the visitor approves each transaction in the wallet. The minimum of each trade is the quote minus
// a fixed 1% margin, so a small price move between the quote and the block does not refuse it.
import { parseAccount, parseSellQuote } from "./account-views.ts";
import { loadBuyQuote } from "./buy-quote.ts";
import type { ChainReader } from "./chain.ts";
import { buildBuy, buildSell, readBuyerStorage } from "./tx-builders.ts";
import { REASON_SENTENCE } from "./your-plan-model.ts";
import { parseTxOutcome, type TxResult } from "./tx-outcome.ts";
import type { TxRequest, WalletSession } from "./tx-types.ts";

/** The quote minus the fixed 1% margin, rounded down. */
export function withMargin(quoted: bigint): bigint {
  return (quoted * 99n) / 100n;
}

export type TradeOutcome =
  /** The wallet sent the transaction and the chain gave a final outcome. */
  | { kind: "result"; result: TxResult }
  /** Nothing reached the chain. */
  | { kind: "not-sent"; text: string }
  /** The wallet failed after the visitor's click; the transaction may or may not be on chain. */
  | { kind: "unknown"; text: string };

export interface TradeDeps {
  chain: Pick<ChainReader, "view">;
  session: WalletSession;
  coin: string;
}

const CLOSED = "The wallet closed and sent nothing.";

/** A wallet that closes or refuses rejects with one of these words (near-connect: "User rejected", "Wallet closed"). */
function isClosedWallet(e: unknown): boolean {
  const text = e instanceof Error ? e.message : String(e);
  return /reject|closed|cancel|denied/i.test(text);
}

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** Asks the wallet to send and reads the one outcome. */
export async function sendAndRead(session: WalletSession, txs: TxRequest[]): Promise<TradeOutcome> {
  let outcomes;
  try {
    outcomes = await session.send(txs);
  } catch (e) {
    if (isClosedWallet(e)) return { kind: "not-sent", text: CLOSED };
    return { kind: "unknown", text: `The wallet gave no result (${message(e)}). Check your wallet activity before you try again.` };
  }
  return { kind: "result", result: parseTxOutcome(outcomes[outcomes.length - 1]!) };
}

/** One buy of `nearIn` yoctoNEAR for the signed-in account. */
export async function sendBuy(deps: TradeDeps, nearIn: bigint): Promise<TradeOutcome> {
  const account = deps.session.accountId;
  let txs: TxRequest[];
  try {
    const quote = await loadBuyQuote(deps.chain, account, nearIn);
    if (quote.kind === "unavailable") return { kind: "not-sent", text: "This coin gives no buy quote, so nothing was sent." };
    const storage = await readBuyerStorage(deps.chain, account);
    txs = buildBuy(deps.coin, { nearIn, quote: quote.value, storage, minTokensOut: withMargin(quote.value.tokensOut) });
  } catch (e) {
    return { kind: "not-sent", text: `The buy quote could not load, so nothing was sent. ${message(e)}` };
  }
  return sendAndRead(deps.session, txs);
}


/** One sell of `amount` token base units for the signed-in account. Nothing is sent when no part can sell now. */
export async function sendSell(deps: TradeDeps, amount: bigint): Promise<TradeOutcome> {
  const account = deps.session.accountId;
  let txs: TxRequest[];
  try {
    const quote = parseSellQuote(await deps.chain.view("fc_quote_sell", { account_id: account, amount: amount.toString() }));
    if (quote.tokensFilled === 0n) {
      const why = quote.limitReason === null ? "" : ` ${REASON_SENTENCE[quote.limitReason]}`;
      return { kind: "not-sent", text: `None of it can sell now, so nothing was sent.${why}` };
    }
    // For a quote that fills in part, near_out is the NEAR of the part that fills: the margin applies to that.
    txs = buildSell(deps.coin, { amount, quote, minNearOut: withMargin(quote.nearOut) });
  } catch (e) {
    return { kind: "not-sent", text: `The sell quote could not load, so nothing was sent. ${message(e)}` };
  }
  return sendAndRead(deps.session, txs);
}

export interface SellLimit {
  balance: bigint;
  /** The tokens of the whole balance that fill now, from fc_quote_sell. */
  canSellNow: bigint;
}

/** What the account holds, and how much of it the coin says can sell now. */
export async function loadSellLimit(chain: Pick<ChainReader, "view">, accountId: string): Promise<SellLimit> {
  const { balance } = parseAccount(await chain.view("fc_account", { account_id: accountId }));
  if (balance === 0n) return { balance, canSellNow: 0n };
  const quote = parseSellQuote(await chain.view("fc_quote_sell", { account_id: accountId, amount: balance.toString() }));
  return { balance, canSellNow: quote.tokensFilled };
}
