// Makes the file the page's "Recorded testnet run" view reads, from the run evidence file.
//   node scripts/build-recorded-run.ts <evidence/testnet-run.json> [out file]   (default: public/recorded-run.json)
// The output holds only the fields the page reads: network, coin and, per transaction, the hash,
// outcome, account and sell amounts or refusal code. Nothing else of the evidence file is copied.
// The output must pass the page's own reader, so a file the page would refuse is never written.
import { readFileSync, writeFileSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { KNOWN_NETWORKS, isNetworkName } from "../src/known-deployments.ts";
import { parseRecordedRun } from "../src/recorded-run.ts";

const TX_FIELDS = ["tx_hash", "outcome", "account", "tokens_asked", "tokens_sold", "near_out", "refusal_code"] as const;

export interface RecordedRunFile {
  network: string;
  coin: string;
  transactions: Record<string, string>[];
}

export function buildRecordedRun(evidence: unknown): RecordedRunFile {
  if (typeof evidence !== "object" || evidence === null) throw new Error("the evidence is not an object");
  const e = evidence as Record<string, unknown>;
  // The network and the coin must be the fixed ones. Nothing else of the evidence goes into a public file.
  if (typeof e.network !== "string" || !isNetworkName(e.network)) throw new Error("the evidence is not for a known network");
  const known = KNOWN_NETWORKS[e.network];
  if (known.demoCoin === null || e.coin !== known.demoCoin) throw new Error("the evidence is not for the demo coin of that network");
  if (!Array.isArray(e.transactions)) throw new Error("the evidence has no transactions list");
  const transactions = e.transactions.map((tx): Record<string, string> => {
    const t = (typeof tx === "object" && tx !== null ? tx : {}) as Record<string, unknown>;
    const kept: Record<string, string> = {};
    for (const field of TX_FIELDS) if (typeof t[field] === "string") kept[field] = t[field];
    return kept;
  });
  const file: RecordedRunFile = { network: e.network, coin: known.demoCoin, transactions };
  const checked = parseRecordedRun(file, { network: file.network, coin: file.coin, explorerTxUrl: known.explorerTxUrl });
  if (!checked.ok) throw new Error(`the page would refuse this run: ${checked.reason}`);
  return file;
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const [input, output = new URL("../public/recorded-run.json", import.meta.url).pathname] = process.argv.slice(2);
  if (!input) {
    console.error("usage: node scripts/build-recorded-run.ts <evidence file> [out file]");
    process.exit(2);
  }
  try {
    const file = buildRecordedRun(JSON.parse(readFileSync(input, "utf8")));
    writeFileSync(output, `${JSON.stringify(file, null, 2)}\n`);
    console.log(`build-recorded-run: wrote ${file.transactions.length} transactions to ${output}`);
  } catch (err) {
    console.error(`build-recorded-run: FAIL ${err instanceof Error ? err.message : String(err)}`);
    process.exit(1);
  }
}
