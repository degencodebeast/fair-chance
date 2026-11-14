// Reads the final outcome of one signed transaction into the result the page shows. Pure. The
// numbers come from the coin's own events (fc_buy, fc_sell, fc_creator_claim) and the factory's
// fc_launch event; a refusal sentence comes from the shared refusal text file.
import type { LimitReason, SellFill } from "./account-views.ts";
import { cleanChainText, cleanChainValue } from "./chain-text.ts";
import { KNOWN_NETWORKS } from "./known-deployments.ts";
import { refusalTextForMessage } from "./refusal-texts.ts";
import type { FinalOutcome } from "./tx-types.ts";

interface Base {
  txHash: string;
  explorerUrl: string;
}

export type TxResult = Base &
  (
  | {
    kind: "bought";
    account: string;
    nearIn: bigint;
    tokensOut: bigint;
    /** Early-supply tokens of this buy (all of them in the early blocks or for the creator group). */
    markedTokens: bigint;
    feeNear: bigint;
  }
  | {
    kind: "sold";
    account: string;
    fill: SellFill;
    tokensAsked: bigint;
    tokensSold: bigint;
    /** Tokens asked that did not sell and stay in the wallet. */
    tokensKept: bigint;
    nearOut: bigint;
    penaltyNear: bigint;
    limitReason: LimitReason | null;
  }
  | {
    /** The factory made the coin. The page can show it at once from these fields. */
    kind: "launched";
    coin: string;
    creator: string;
    launchBlock: number;
    description: string | null;
    /** Tokens of the creator's opening buy; 0 when the launch had none. */
    openingTokens: bigint;
  }
  | {
    /** The creator fee transfer succeeded. */
    kind: "claimed";
    creator: string;
    amount: bigint;
  }
  | {
    /** The contract refused the call; nothing changed and any attached NEAR came back. */
    kind: "refused";
    /** The FC_ code, or null for a standard NEP-141 or NEP-145 message. */
    code: string | null;
    /** The sentence from the shared refusal text file. */
    text: string;
    chainMessage: string;
  }
  | {
    /** A receipt failed with a message the refusal file does not know, or no Fair Chance event came back. */
    kind: "failed";
    chainMessage: string;
  }
  );

/** The testnet explorer page of a transaction. */
export function explorerTxLink(txHash: string, explorerTxUrl: string = KNOWN_NETWORKS.testnet.explorerTxUrl): string {
  return `${explorerTxUrl}${txHash}`;
}

interface LoggedEvent {
  standard: string;
  event: string;
  data: Record<string, unknown>;
}

const EVENT_PREFIX = "EVENT_JSON:";

/** Every NEP-297 event a receipt of the transaction logged. */
function events(outcome: FinalOutcome): LoggedEvent[] {
  const found: LoggedEvent[] = [];
  for (const receipt of outcome.receipts_outcome) {
    for (const log of receipt.outcome.logs) {
      if (!log.startsWith(EVENT_PREFIX)) continue;
      const parsed = cleanChainValue(JSON.parse(log.slice(EVENT_PREFIX.length))) as { standard: string; event: string; data: Record<string, unknown>[] };
      for (const data of parsed.data) found.push({ standard: parsed.standard, event: parsed.event, data });
    }
  }
  return found;
}

/**
 * The message of the first failed receipt, or of the failed transaction itself; null when nothing
 * failed. A contract panic gives its ExecutionError text; any other failure gives its JSON.
 */
function firstFailure(outcome: FinalOutcome): string | null {
  const statuses = [outcome.transaction_outcome.outcome.status, ...outcome.receipts_outcome.map((r) => r.outcome.status), outcome.status];
  for (const status of statuses) {
    if (typeof status !== "object" || status === null || !("Failure" in status)) continue;
    const failure = (status as { Failure: unknown }).Failure;
    return cleanChainText(executionError(failure) ?? JSON.stringify(failure));
  }
  return null;
}

/** The ExecutionError text inside ActionError.kind.FunctionCallError, if there is one. */
function executionError(failure: unknown): string | null {
  const kind = (failure as { ActionError?: { kind?: { FunctionCallError?: { ExecutionError?: unknown } } } })?.ActionError?.kind;
  const text = kind?.FunctionCallError?.ExecutionError;
  return typeof text === "string" ? text : null;
}

const amount = (data: Record<string, unknown>, key: string): bigint => {
  const value = data[key];
  if (typeof value !== "string" || !/^\d+$/.test(value)) throw new Error(`event field ${key} is not an amount`);
  return BigInt(value);
};

const NO_EVENT = "The transaction succeeded but logged no Fair Chance event.";

const find = (all: LoggedEvent[], standard: string, event: string) => all.find((e) => e.standard === standard && e.event === event);

/**
 * The result of one transaction. A failed receipt wins over any event: a refusal when the shared
 * refusal file knows the message, else "failed" with the chain's text. A launch whose coin start
 * failed is "failed" too (the factory refunds the deposit). The explorer link uses the testnet
 * explorer unless `explorerTxUrl` is given.
 */
export function parseTxOutcome(outcome: FinalOutcome, explorerTxUrl?: string): TxResult {
  const txHash = outcome.transaction.hash;
  const base: Base = { txHash, explorerUrl: explorerTxLink(txHash, explorerTxUrl) };
  const failure = firstFailure(outcome);
  if (failure !== null) {
    const text = refusalTextForMessage(failure);
    if (text === null) return { ...base, kind: "failed", chainMessage: failure };
    return { ...base, kind: "refused", code: failure.match(/FC_[A-Z_]+/)?.[0] ?? null, text, chainMessage: failure };
  }
  const all = events(outcome);
  const sell = find(all, "fair_chance", "fc_sell");
  if (sell) {
    const d = sell.data;
    const tokensAsked = amount(d, "tokens_asked");
    const tokensSold = amount(d, "tokens_sold");
    const reason = d.limit_reason;
    return {
      ...base,
      kind: "sold",
      account: String(d.account),
      fill: d.fill as SellFill,
      tokensAsked,
      tokensSold,
      tokensKept: tokensAsked - tokensSold,
      nearOut: amount(d, "near_out"),
      penaltyNear: amount(d, "penalty_near"),
      limitReason: reason === null || reason === undefined ? null : (reason as LimitReason),
    };
  }
  // A launch with an opening buy also logs the creator's fc_buy, so the launch is read first.
  const launch = find(all, "fair_chance_factory", "fc_launch");
  const buy = find(all, "fair_chance", "fc_buy");
  if (launch) {
    const d = launch.data;
    return {
      ...base,
      kind: "launched",
      coin: String(d.coin),
      creator: String(d.creator),
      launchBlock: Number(d.launch_block),
      description: typeof d.description === "string" ? d.description : null,
      openingTokens: buy ? amount(buy.data, "tokens_out") : 0n,
    };
  }
  if (buy) {
    const d = buy.data;
    return {
      ...base,
      kind: "bought",
      account: String(d.account),
      nearIn: amount(d, "near_in"),
      tokensOut: amount(d, "tokens_out"),
      markedTokens: amount(d, "marked_tokens"),
      feeNear: amount(d, "fee_near"),
    };
  }
  const claim = find(all, "fair_chance", "fc_creator_claim");
  if (claim) return { ...base, kind: "claimed", creator: String(claim.data.creator), amount: amount(claim.data, "amount") };
  return { ...base, kind: "failed", chainMessage: NO_EVENT };
}
