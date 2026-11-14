// Event source for NEAR testnet: the FastNEAR transactions API, read only.
// POST /v0/account lists the transactions that logged events for the coin. POST /v0/transactions
// gives up to 20 of them with their receipt logs. The public endpoint needs no key. A 403 or any
// answer this code does not know stops the read with an error and is never retried. A 429, a 5xx
// or no readable answer gets the short retry of chain-fetch.ts.
import { readWithRetry, type RetryOptions } from "./chain-fetch.ts";
import { httpStatusError } from "./refresher.ts";
import { parseLog, type CoinEvent } from "./chain.ts";

const ACCOUNT_PAGE_SIZE = 100;
const ACCOUNT_MAX_PAGES = 10;
const TRANSACTIONS_PER_CALL = 20;

interface AccountTx {
  transaction_hash: string;
  tx_block_height: number;
  tx_block_timestamp?: string;
}

interface FastNearTransaction {
  transaction: { hash: string };
  receipts: {
    receipt: { receipt_id: string; receiver_id: string };
    execution_outcome: { outcome: { logs: string[]; status: Record<string, unknown> } };
  }[];
}

export interface FastNearRead {
  baseUrl: string;
  coin: string;
  /** First block height to include. */
  since: number;
  fetchFn: typeof fetch;
  /**
   * Leave out FastNEAR's is_event_log filter. A coin logs its events in its own call, so the
   * filter is right and cheap for a coin. The factory logs fc_launch in a callback receipt, which
   * FastNEAR does not flag, so the factory read needs every successful transaction. The receipt
   * logs are still parsed and filtered by the code below.
   */
  anyLoggedTx?: boolean;
  /** Tests give their own limiter and wait. */
  retry?: RetryOptions;
}

export async function fastNearEvents(read: FastNearRead): Promise<{ events: CoinEvent[]; cursor: number }> {
  const txs = (await accountTxsSince(read)).sort((a, b) => a.tx_block_height - b.tx_block_height);
  const out: CoinEvent[] = [];
  const seen = new Set<string>();
  for (let start = 0; start < txs.length; start += TRANSACTIONS_PER_CALL) {
    const batch = txs.slice(start, start + TRANSACTIONS_PER_CALL);
    const res = await post<{ transactions: FastNearTransaction[] }>(read, "/v0/transactions", {
      tx_hashes: batch.map((t) => t.transaction_hash),
    });
    for (const tx of res.transactions) {
      const asked = batch.find((t) => t.transaction_hash === tx.transaction.hash);
      if (!asked) continue;
      for (const r of tx.receipts) {
        const id = r.receipt.receipt_id;
        const status = r.execution_outcome.outcome.status;
        if (r.receipt.receiver_id !== read.coin || seen.has(id) || !("SuccessValue" in status || "SuccessReceiptId" in status)) {
          continue;
        }
        seen.add(id);
        for (const line of r.execution_outcome.outcome.logs) {
          const ev = parseLog(line);
          if (ev) {
            out.push({ ...ev, coin: read.coin, receiptId: id, block: asked.tx_block_height, timeNs: asked.tx_block_timestamp ?? null });
          }
        }
      }
    }
  }
  const newest = txs.reduce((height, t) => Math.max(height, t.tx_block_height + 1), read.since);
  return { events: out.sort((a, b) => a.block - b.block), cursor: newest };
}

/** Pages through the newest-first list until it reaches a transaction below `since`. */
async function accountTxsSince(read: FastNearRead): Promise<AccountTx[]> {
  const kept: AccountTx[] = [];
  let token: string | null = null;
  for (let page = 0; page < ACCOUNT_MAX_PAGES; page++) {
    const body: Record<string, unknown> = {
      account_id: read.coin,
      desc: true,
      ...(read.anyLoggedTx ? {} : { is_event_log: true }),
      is_success: true,
      limit: ACCOUNT_PAGE_SIZE,
    };
    if (token) body.resume_token = token;
    const res = await post<{ account_txs: AccountTx[]; resume_token: string | null }>(read, "/v0/account", body);
    let reachedOld = false;
    for (const tx of res.account_txs) {
      if (tx.tx_block_height < read.since) reachedOld = true;
      else kept.push(tx);
    }
    if (reachedOld || !res.resume_token || res.account_txs.length === 0) return kept;
    token = res.resume_token;
  }
  throw new Error(`more than ${ACCOUNT_MAX_PAGES} pages of transactions for ${read.coin} since block ${read.since}`);
}

async function post<T>(read: FastNearRead, path: string, body: object): Promise<T> {
  // Call fetch with no receiver: window.fetch throws "Illegal invocation" when called as a method of `read`.
  const { fetchFn } = read;
  return readWithRetry(async () => {
    const resp = await fetchFn(read.baseUrl + path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (resp.status === 403) throw new Error(`${path}: FastNEAR answered 403, stop and report`);
    if (resp.status === 429) throw httpStatusError(`${path}: FastNEAR answered 429, wait before the next read`, resp);
    if (!resp.ok) throw httpStatusError(`${path}: HTTP ${resp.status}, an answer this page does not know`, resp);
    return (await resp.json()) as T;
  }, read.retry);
}
