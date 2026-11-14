// The plain words of a trade result. Pure. Numbers come from the coin's own events in the final
// outcome; a refusal sentence comes from the shared refusal text file through the outcome parser.
import { compactTokens, nearAmount } from "./format.ts";
import type { TradeOutcome } from "./trade-actions.ts";
import { REASON_SENTENCE } from "./your-plan-model.ts";

export interface TradeResultText {
  tone: "ok" | "limit" | "refused";
  headline: string;
  lines: string[];
  /** The testnet explorer page of the transaction; null when nothing reached the chain. */
  explorerUrl: string | null;
}

const near = (yocto: bigint) => `${nearAmount(yocto.toString(), 4)} NEAR`;

export function tradeResultText(outcome: TradeOutcome, coin: { symbol: string; decimals: number }): TradeResultText {
  if (outcome.kind !== "result") return { tone: "refused", headline: outcome.text, lines: [], explorerUrl: null };
  const r = outcome.result;
  const tokens = (units: bigint) => `${compactTokens(units.toString(), coin.decimals)} ${coin.symbol}`;
  const base = { explorerUrl: r.explorerUrl };
  switch (r.kind) {
    case "bought": {
      const lines = [`Swap fee: ${near(r.feeNear)}.`];
      if (r.markedTokens > 0n) lines.push(`Early supply tokens in this buy: ${tokens(r.markedTokens)}.`);
      return { ...base, tone: "ok", headline: `You bought ${tokens(r.tokensOut)} for ${near(r.nearIn)}.`, lines };
    }
    case "sold": {
      const lines: string[] = [];
      if (r.penaltyNear > 0n) lines.push(`Penalty: ${near(r.penaltyNear)}.`);
      if (r.tokensKept === 0n) return { ...base, tone: "ok", headline: `You sold ${tokens(r.tokensSold)} for ${near(r.nearOut)}.`, lines };
      const sold = compactTokens(r.tokensSold.toString(), coin.decimals);
      lines.unshift(`${tokens(r.tokensKept)} stay in your wallet.`);
      if (r.limitReason !== null) lines.push(REASON_SENTENCE[r.limitReason]);
      return { ...base, tone: "limit", headline: `You sold ${sold} of ${tokens(r.tokensAsked)} for ${near(r.nearOut)}.`, lines };
    }
    case "launched":
      return { ...base, tone: "ok", headline: `The factory launched ${r.coin}.`, lines: [] };
    case "claimed":
      return { ...base, tone: "ok", headline: `You claimed ${near(r.amount)} of creator fees.`, lines: [] };
    case "refused":
      return { ...base, tone: "refused", headline: "The coin refused this transaction, so nothing changed.", lines: [r.text] };
    case "failed":
      return { ...base, tone: "refused", headline: "The transaction failed.", lines: [r.chainMessage] };
  }
}
