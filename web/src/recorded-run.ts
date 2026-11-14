// The "Recorded testnet run" receipts: transactions from one finished testnet run, read from a
// same-site evidence file named in known-deployments.ts. The page takes only facts from the file.
// The explorer link comes from the fixed config, the refusal text from the shared refusal file,
// and the coin must be the coin the page shows. Nothing here is live chain data.
//
// File shape (JSON):
//   { "network": "testnet", "coin": "<coin account>",
//     "transactions": [
//       { "tx_hash": "<base58>", "outcome": "partial_fill" | "full_fill", "account": "<account>",
//         "tokens_asked": "<base units>", "tokens_sold": "<base units>", "near_out": "<yoctoNEAR>" },
//       { "tx_hash": "<base58>", "outcome": "refusal", "account": "<account>", "refusal_code": "FC_..." } ] }
import { KNOWN_NETWORKS, isNetworkName, type KnownNetwork } from "./known-deployments.ts";
import { nearShort, tokensPair } from "./receipts.ts";
import { refusalText } from "./refusal-texts.ts";

export interface RecordedRow {
  txHash: string;
  /** The explorer page of the transaction, built from the fixed config. */
  link: string;
  text: string;
}

export type RecordedRun = { ok: true; rows: RecordedRow[] } | { ok: false; reason: string };

const TX_HASH = /^[1-9A-HJ-NP-Za-km-z]{43,44}$/;
const UNITS = /^\d+$/;
const ACCOUNT = /^[a-z0-9._-]{2,64}$/;

function fail(reason: string): RecordedRun {
  return { ok: false, reason };
}

function row(tx: unknown, explorerTxUrl: string): RecordedRow | string {
  if (typeof tx !== "object" || tx === null) return "a transaction is not an object";
  const t = tx as Record<string, unknown>;
  const text = (name: string) => (typeof t[name] === "string" ? (t[name] as string) : "");
  const hash = text("tx_hash");
  if (!TX_HASH.test(hash)) return "a transaction hash is not valid";
  const account = text("account");
  if (!ACCOUNT.test(account)) return "an account name is not valid";
  const link = `${explorerTxUrl}${hash}`;
  const outcome = text("outcome");
  if (outcome === "refusal") {
    const reason = refusalText(text("refusal_code"));
    if (reason === null) return "a refusal code has no text in the shared refusal file";
    return { txHash: hash, link, text: `${account} was refused. ${reason}` };
  }
  if (outcome !== "partial_fill" && outcome !== "full_fill") return "an outcome is not known";
  const [asked, sold, out] = [text("tokens_asked"), text("tokens_sold"), text("near_out")];
  if (![asked, sold, out].every((n) => UNITS.test(n))) return "an amount is not a whole number";
  const [askedUnits, soldUnits] = [BigInt(asked), BigInt(sold)];
  if (outcome === "full_fill" ? soldUnits !== askedUnits : soldUnits <= 0n || soldUnits >= askedUnits) return "the sold amount does not fit the outcome";
  const [soldText, askedText] = tokensPair(sold, asked);
  const words = outcome === "full_fill" ? "It filled in full." : "It filled in part.";
  return { txHash: hash, link, text: `${account} sold ${soldText} of ${askedText} tokens for ${nearShort(out)} NEAR. ${words}` };
}

/** Checks the file and makes the rows. Any bad entry fails the whole file: a run is shown whole or not at all. */
export function parseRecordedRun(file: unknown, expect: { network: string; coin: string; explorerTxUrl: string }): RecordedRun {
  if (typeof file !== "object" || file === null) return fail("the file is not a run");
  const f = file as Record<string, unknown>;
  if (f.network !== expect.network) return fail("the file is for another network");
  if (f.coin !== expect.coin) return fail("the file is for another coin");
  if (!Array.isArray(f.transactions) || f.transactions.length === 0) return fail("the file has no transactions");
  const rows: RecordedRow[] = [];
  const seen = new Set<string>();
  for (const tx of f.transactions) {
    const made = row(tx, expect.explorerTxUrl);
    if (typeof made === "string") return fail(made);
    if (seen.has(made.txHash)) return fail("a transaction hash is repeated");
    seen.add(made.txHash);
    rows.push(made);
  }
  return { ok: true, rows };
}

/**
 * Reads the recorded run for the coin in the page link. Null when there is nothing to show: the link
 * names no testnet coin, a coin other than the demo coin, or the config has no file yet. A missing or broken file is a failure, so the
 * page can say so instead of showing an empty run.
 */
export async function loadRecordedRun(href: string, network: KnownNetwork | null = networkFor(href), fetchFn: typeof fetch = fetch): Promise<RecordedRun | null> {
  const url = new URL(href);
  const coin = url.searchParams.get("coin");
  const networkName = url.searchParams.get("network");
  // The file holds the run of the demo coin only, so any other coin shows no recorded run at all.
  if (network === null || networkName === null || network.recordedRunFile === null || !coin || coin !== network.demoCoin) return null;
  try {
    const response = await fetchFn(new URL(network.recordedRunFile, url.origin + "/").href);
    if (!response.ok) return fail(`the file did not load (HTTP ${response.status})`);
    return parseRecordedRun(await response.json(), { network: networkName, coin, explorerTxUrl: network.explorerTxUrl });
  } catch {
    return fail("the file did not load");
  }
}

function networkFor(href: string): KnownNetwork | null {
  const name = new URL(href).searchParams.get("network");
  return name !== null && isNetworkName(name) ? KNOWN_NETWORKS[name] : null;
}
