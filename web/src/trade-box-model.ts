// The text of the trade box (buy and sell), as plain data. Pure. Every button that would build or
// send a transaction is turned off in this version and carries the reason the page shows beside it.
// The numbers of a buy preview come from fc_quote_buy; this file does no buy math and does not
// decide "large" itself: it reads `becomes_large` from the view.
import type { Reading } from "./coin-page.ts";
import type { BuyQuote } from "./buy-quote.ts";
import { compactTokens, nearAmount, percentOfBps } from "./format.ts";

export const NOT_AVAILABLE_FROM_COIN = "Not available from this coin";

export interface TurnedOffButton {
  label: string;
  /** Why the button is turned off. Shown next to the button. */
  reason: string;
}

export type BuyPreview =
  | { kind: "none" }
  | { kind: "quote"; accountId: string; nearIn: bigint; quote: Reading<BuyQuote> };

export interface TradeBoxInput {
  symbol: string;
  decimals: number;
  /** `fc_rules.large_threshold_bps`, for the words of the warning. */
  largeThresholdBps: number;
  preview: BuyPreview;
}

export interface TradeBoxModel {
  connect: TurnedOffButton;
  /** Shown above the tabs while no wallet is connected. */
  connectPrompt: string;
  buy: {
    amountLabel: string;
    lines: { label: string; value: string }[];
    /** Set when the coin has no buy quote view. */
    note: string | null;
    largeWarning: string | null;
    button: TurnedOffButton;
  };
  sell: {
    amountLabel: string;
    button: TurnedOffButton;
    /** Where the sell preview is, since this box does not send. */
    pointer: string;
  };
}

const WALLET_REASON = "Wallet connect is turned off in this version. Nothing on this page signs or sends a transaction.";
const BUY_REASON =
  "Buying is turned off in this version. A buy from an account with no storage record also pays the storage deposit, in the same transaction.";
const SELL_REASON = "Selling is turned off in this version. A sell needs your wallet to confirm it with 1 yoctoNEAR attached.";

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
    connect: { label: "Connect", reason: WALLET_REASON },
    connectPrompt: "Connect a NEAR wallet to buy or sell",
    buy: { amountLabel: "You pay (NEAR)", lines, note, largeWarning, button: { label: buyLabel, reason: BUY_REASON } },
    sell: {
      amountLabel: `Amount to sell (${symbol})`,
      button: { label: "Sell", reason: SELL_REASON },
      pointer: "To preview a sell, type an account and an amount in Your plan.",
    },
  };
}
