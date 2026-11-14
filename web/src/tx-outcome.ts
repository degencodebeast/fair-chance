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
  /** Null when the hash is not a NEAR transaction hash. */
  explorerUrl: string | null;
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
/** A NEAR transaction hash: 32 bytes in base58, 43 or 44 characters. */
const TX_HASH = /^[1-9A-HJ-NP-Za-km-z]{43,44}$/;

/** The testnet explorer page of a transaction; null for anything that is not a transaction hash. */
export function explorerTxLink(txHash: string, explorerTxUrl: string = KNOWN_NETWORKS.testnet.explorerTxUrl): string | null {
  return TX_HASH.test(txHash) ? `${explorerTxUrl}${txHash}` : null;
}

interface LoggedEvent {
  standard: string;
  event: string;
  data: Record<string, unknown>;
}

const EVENT_PREFIX = "EVENT_JSON:";
const UNREADABLE_EVENT = "The transaction result has a Fair Chance event this page cannot read.";
const UNREADABLE_RESULT = "The transaction result cannot be read.";
const NO_EVENT = "The transaction succeeded but logged no Fair Chance event.";

/** A Fair Chance event, from the contract that should log it, that does not have the interface's shape. */
class UnreadableEvent extends Error {}

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/**
 * Every NEP-297 event logged by receipts that `executor` ran. Logs of other accounts are never
 * read. A log of this account that is not an event list of objects is unreadable.
 */
function eventsOf(outcome: FinalOutcome, executor: string): LoggedEvent[] {
  const found: LoggedEvent[] = [];
  for (const receipt of outcome.receipts_outcome) {
    if (receipt.outcome.executor_id !== executor) continue;
    for (const log of receipt.outcome.logs) {
      if (!log.startsWith(EVENT_PREFIX)) continue;
      let parsed: unknown;
      try {
        parsed = cleanChainValue(JSON.parse(log.slice(EVENT_PREFIX.length)));
      } catch {
        throw new UnreadableEvent();
      }
      if (!isRecord(parsed) || typeof parsed.standard !== "string" || typeof parsed.event !== "string" || !Array.isArray(parsed.data)) throw new UnreadableEvent();
      for (const data of parsed.data) {
        if (!isRecord(data)) throw new UnreadableEvent();
        found.push({ standard: parsed.standard, event: parsed.event, data });
      }
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
    if (!isRecord(status) || !("Failure" in status)) continue;
    return cleanChainText(executionError(status.Failure) ?? JSON.stringify(status.Failure));
  }
  return null;
}

/** The ExecutionError text inside ActionError.kind.FunctionCallError, if there is one. */
function executionError(failure: unknown): string | null {
  const kind = (failure as { ActionError?: { kind?: { FunctionCallError?: { ExecutionError?: unknown } } } })?.ActionError?.kind;
  const text = kind?.FunctionCallError?.ExecutionError;
  return typeof text === "string" ? text : null;
}

function amount(data: Record<string, unknown>, key: string): bigint {
  const value = data[key];
  if (typeof value !== "string" || !/^\d+$/.test(value)) throw new UnreadableEvent();
  return BigInt(value);
}

function text(data: Record<string, unknown>, key: string): string {
  const value = data[key];
  if (typeof value !== "string") throw new UnreadableEvent();
  return value;
}

const FILLS: readonly SellFill[] = ["full", "partial", "none"];
const LIMIT_REASONS: readonly LimitReason[] = ["allowance", "cap_full", "floor"];

const find = (all: LoggedEvent[], standard: string, event: string) => all.find((e) => e.standard === standard && e.event === event);

/**
 * The result of one transaction. Never throws. A failed receipt wins over any event: a refusal
 * when the shared refusal file knows the message, else "failed" with the chain's text. Events are
 * read only from the account the transaction called (the coin, or the factory for a launch) and,
 * for a launch, from the new coin, which must be a direct sub-account of that factory. A launch
 * whose coin start failed is "failed" (the factory refunds the deposit). The explorer link uses
 * the testnet explorer unless `explorerTxUrl` is given.
 */
export function parseTxOutcome(outcome: FinalOutcome, explorerTxUrl?: string): TxResult {
  const hash = (outcome as { transaction?: { hash?: unknown } } | null)?.transaction?.hash;
  const txHash = typeof hash === "string" ? hash : "";
  const base: Base = { txHash, explorerUrl: explorerTxLink(txHash, explorerTxUrl) };
  try {
    return read(outcome, base);
  } catch (e) {
    return { ...base, kind: "failed", chainMessage: e instanceof UnreadableEvent ? UNREADABLE_EVENT : UNREADABLE_RESULT };
  }
}

function read(outcome: FinalOutcome, base: Base): TxResult {
  const failure = firstFailure(outcome);
  if (failure !== null) {
    const refusal = refusalTextForMessage(failure);
    if (refusal === null) return { ...base, kind: "failed", chainMessage: failure };
    return { ...base, kind: "refused", code: failure.match(/FC_[A-Z_]+/)?.[0] ?? null, text: refusal, chainMessage: failure };
  }
  const target = outcome.transaction.receiver_id;
  const fromTarget = eventsOf(outcome, target);
  const launch = find(fromTarget, "fair_chance_factory", "fc_launch");
  if (launch) {
    const d = launch.data;
    const coin = text(d, "coin");
    if (!coin.endsWith(`.${target}`) || coin.slice(0, -target.length - 1).includes(".")) throw new UnreadableEvent();
    const launchBlock = d.launch_block;
    if (typeof launchBlock !== "number" || !Number.isSafeInteger(launchBlock)) throw new UnreadableEvent();
    if (d.description !== null && d.description !== undefined && typeof d.description !== "string") throw new UnreadableEvent();
    // A launch with an opening buy also logs the creator's fc_buy, from the new coin.
    const openingBuy = find(eventsOf(outcome, coin), "fair_chance", "fc_buy");
    return {
      ...base,
      kind: "launched",
      coin,
      creator: text(d, "creator"),
      launchBlock,
      description: typeof d.description === "string" ? d.description : null,
      openingTokens: openingBuy ? amount(openingBuy.data, "tokens_out") : 0n,
    };
  }
  const sell = find(fromTarget, "fair_chance", "fc_sell");
  if (sell) {
    const d = sell.data;
    const fill = d.fill as SellFill;
    if (!FILLS.includes(fill)) throw new UnreadableEvent();
    const reason = d.limit_reason ?? null;
    if (reason !== null && !LIMIT_REASONS.includes(reason as LimitReason)) throw new UnreadableEvent();
    const tokensAsked = amount(d, "tokens_asked");
    const tokensSold = amount(d, "tokens_sold");
    if (tokensSold > tokensAsked) throw new UnreadableEvent();
    return {
      ...base,
      kind: "sold",
      account: text(d, "account"),
      fill,
      tokensAsked,
      tokensSold,
      tokensKept: tokensAsked - tokensSold,
      nearOut: amount(d, "near_out"),
      penaltyNear: amount(d, "penalty_near"),
      limitReason: reason as LimitReason | null,
    };
  }
  const buy = find(fromTarget, "fair_chance", "fc_buy");
  if (buy) {
    const d = buy.data;
    return {
      ...base,
      kind: "bought",
      account: text(d, "account"),
      nearIn: amount(d, "near_in"),
      tokensOut: amount(d, "tokens_out"),
      markedTokens: amount(d, "marked_tokens"),
      feeNear: amount(d, "fee_near"),
    };
  }
  const claim = find(fromTarget, "fair_chance", "fc_creator_claim");
  if (claim) return { ...base, kind: "claimed", creator: text(claim.data, "creator"), amount: amount(claim.data, "amount") };
  return { ...base, kind: "failed", chainMessage: NO_EVENT };
}
