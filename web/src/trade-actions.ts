// The steps of a buy and a sell from the page: read the coin's quote, build the transaction, ask the
// wallet to sign and send it, and read the result from the final outcome. The page signs nothing:
// the visitor approves each transaction in the wallet. The builders set each minimum from the quote
// with their fixed margin (tx-builders.ts); this file does no price math.
import { parseAccount, parseSellQuote } from "./account-views.ts";
import type { BuyQuote } from "./buy-quote.ts";
import type { ChainReader } from "./chain.ts";
import type { LaunchFormInput } from "./launch-form-model.ts";
import { buildBuy, buildLaunch, buildSell, LAUNCH_STORAGE_DEPOSIT, readBuyerStorage } from "./tx-builders.ts";
import { parseTxOutcome, type TxResult } from "./tx-outcome.ts";
import type { TxRequest, WalletSession } from "./tx-types.ts";
import { REASON_SENTENCE } from "./your-plan-model.ts";

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

/**
 * The two errors @hot-labs/near-connect 0.12 gives when the visitor closes the wallet before it
 * signs: the picker's "User rejected" (NearConnector.js) and the wallet frame's "Wallet closed"
 * (SandboxedWallet/executor.js). Any other error may come after the transaction went out.
 */
const CLOSE_MESSAGES: readonly string[] = ["User rejected", "Wallet closed"];

function isClosedWallet(e: unknown): boolean {
  return e instanceof Error && CLOSE_MESSAGES.includes(e.message);
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

/** The buy the visitor previewed: the NEAR the box showed and the quote the button showed. */
export interface PreviewedBuy {
  nearIn: bigint;
  quote: BuyQuote;
}

/**
 * One buy for the signed-in account, built from the previewed quote and nothing else: the attached
 * NEAR is the previewed amount and the builder sets min_tokens_out from the previewed tokens_out.
 * If the price moved since the preview, the coin refuses with FC_SLIPPAGE and nothing changes.
 */
export async function sendBuy(deps: TradeDeps, buy: PreviewedBuy): Promise<TradeOutcome> {
  let txs: TxRequest[];
  try {
    const storage = await readBuyerStorage(deps.chain, deps.session.accountId);
    txs = buildBuy(deps.coin, { nearIn: buy.nearIn, quote: buy.quote, storage });
  } catch (e) {
    return { kind: "not-sent", text: `The storage record could not load, so nothing was sent. ${message(e)}` };
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
    const sell = buildSell(deps.coin, { amount, quote });
    if (sell.kind === "nothing-fills") return { kind: "not-sent", text: sell.text };
    txs = sell.txs;
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

/** The factory launch from the form, with the fixed storage deposit plus the opening buy. */
export async function sendLaunch(session: WalletSession, factory: string, form: LaunchFormInput, icon: string | null): Promise<TradeOutcome> {
  const launch = buildLaunch(factory, { form, icon, storageDeposit: LAUNCH_STORAGE_DEPOSIT });
  if (launch.kind === "invalid") return { kind: "not-sent", text: launch.errors.join(" ") };
  return sendAndRead(session, launch.txs);
}
