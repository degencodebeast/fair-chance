// The text of the trade box (buy and sell), as plain data. Pure. A button that cannot act now is
// turned off and carries the reason the page shows beside it; the wallet signs, never the page.
// The numbers of a buy preview come from fc_quote_buy; this file does no buy math and does not
// decide "large" itself: it reads `becomes_large` from the view.
import type { Reading } from "./coin-page.ts";
import type { BuyQuote } from "./buy-quote.ts";
import { compactTokens, exactNear, nearAmount, percentOfBps } from "./format.ts";

export const NOT_AVAILABLE_FROM_COIN = "Not available from this coin";

export interface TradeButton {
  label: string;
  /** Why the button is turned off, shown next to it; null when it is on. */
  reason: string | null;
}

export type BuyPreview =
  | { kind: "none" }
  | { kind: "quote"; accountId: string; nearIn: bigint; quote: Reading<BuyQuote> };

export interface TradeBoxInput {
  symbol: string;
  decimals: number;
  /** `fc_rules.large_threshold_bps`, for the words of the warning. */
  largeThresholdBps: number;
  /** `storage_balance_bounds().min` of the coin, for the buy reason. */
  storageMin: Reading<bigint>;
  preview: BuyPreview;
  /** The signed-in testnet account; null when no wallet is connected. */
  signedIn: string | null;
  /** True while the wallet is open for a buy or a sell. */
  pending: boolean;
  /** The warning when the coin fails the page's own chain checks: then no trade turns on. Null when every check passes. */
  blocked: string | null;
}

export interface TradeBoxModel {
  connect: { label: string };
  /** Shown above the tabs while no wallet is connected. */
  connectPrompt: string;
  buy: {
    amountLabel: string;
    lines: { label: string; value: string }[];
    /** Set when the coin has no buy quote view. */
    note: string | null;
    largeWarning: string | null;
    /** What a first buy also pays for the account's storage record. */
    storageNote: string;
    button: TradeButton;
  };
  sell: {
    amountLabel: string;
    button: TradeButton;
  };
}

function storageNote(storageMin: Reading<bigint>): string {
  const deposit =
    storageMin.kind === "value"
      ? `a storage deposit of ${exactNear(storageMin.value.toString())} NEAR.`
      : "a storage deposit. The amount is not available from this coin.";
  return `If this account never held this coin, the buy also pays ${deposit}`;
}
const WAITING: TradeButton = { label: "Waiting for the wallet", reason: "Approve or close the transaction in your wallet." };

function buyButton(input: TradeBoxInput, label: string): TradeButton {
  if (input.blocked !== null) return { label, reason: input.blocked };
  if (input.pending) return WAITING;
  if (input.signedIn === null) return { label, reason: "Connect a wallet to buy." };
  const p = input.preview;
  if (p.kind !== "quote" || p.quote.kind !== "value" || p.accountId !== input.signedIn) return { label, reason: `Preview the buy for ${input.signedIn} first.` };
  return { label, reason: null };
}

function sellButton(input: TradeBoxInput): TradeButton {
  if (input.blocked !== null) return { label: "Sell", reason: input.blocked };
  if (input.pending) return WAITING;
  if (input.signedIn === null) return { label: "Sell", reason: "Connect a wallet to sell." };
  return { label: "Sell", reason: null };
}

export function tradeBoxModel(input: TradeBoxInput): TradeBoxModel {
  const { symbol, decimals } = input;
  let lines: { label: string; value: string }[] = [];
  let note: string | null = null;
  let largeWarning: string | null = null;
  let buyLabel = "Buy";
  const p = input.preview;
  if (p.kind === "quote") {
    if (p.quote.kind === "unavailable") {
      note = NOT_AVAILABLE_FROM_COIN;
    } else {
      const q = p.quote.value;
      const got = compactTokens(q.tokensOut.toString(), decimals);
      buyLabel = `Buy ${got} ${symbol}`;
      lines = [
        { label: "You get about", value: `${got} ${symbol}` },
        { label: "Swap fee, kept in the pool and creator fees", value: `${nearAmount(q.feeNear.toString(), 3)} NEAR` },
        { label: "Of that, to the creator fee balance", value: `${nearAmount(q.creatorFeeNear.toString(), 3)} NEAR` },
      ];
      if (q.markedTokens > 0n) lines.push({ label: "Early supply tokens in this buy", value: `${compactTokens(q.markedTokens.toString(), decimals)} ${symbol}` });
      lines.push({ label: "Allowance after this buy", value: `${compactTokens(q.allowanceAfter.toString(), decimals)} ${symbol} per hour` });
      if (q.becomesLarge) {
        largeWarning =
          `This buy makes you a large holder (${percentOfBps(input.largeThresholdBps)} of supply or more). ` +
          "A large holder cannot send tokens until the balance is zero. " +
          `After this buy your allowance is ${compactTokens(q.allowanceAfter.toString(), decimals)} ${symbol} per hour, less when the pool cap is full.`;
      }
    }
  }
  return {
    connect: { label: "Connect wallet" },
    connectPrompt: "Connect a NEAR testnet wallet to buy or sell",
    buy: { amountLabel: "You pay (NEAR)", lines, note, largeWarning, storageNote: storageNote(input.storageMin), button: buyButton(input, buyLabel) },
    sell: {
      amountLabel: `Amount to sell (${symbol})`,
      button: sellButton(input),
    },
  };
}
