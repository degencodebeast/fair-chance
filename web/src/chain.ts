// Chain-read module: views and events for one coin, from a NEAR RPC endpoint.
// The page reads the chain only through this module, never through the watcher.
import { fastNearEvents } from "./fastnear-events.ts";

export interface ReaderConfig {
  rpcUrl: string;
  coin: string;
  /** Base URL of the FastNEAR transactions API. Set: events come from it (testnet). Unset: block scan. */
  fastNearUrl?: string;
}

export const FASTNEAR_TESTNET_URL = "https://tx.test.fastnear.com";

export interface CoinEvent {
  standard: "fair_chance" | "fair_chance_factory";
  version: string;
  name: string;
  coin: string;
  receiptId: string;
  /** Height of the block that included the transaction. */
  block: number;
  /** Time of that block in nanoseconds, as a decimal string; null when the source gives none. */
  timeNs: string | null;
  data: Record<string, unknown>;
}

/** What `view_account` says about an account. Only the fields the page checks. */
export interface AccountFacts {
  /** Hash of a global contract the account uses by hash; null when it uses none. */
  globalContractHash: string | null;
  /** The account uses a global contract by account ID, so that account can change its code. */
  usesGlobalContractByAccount: boolean;
  /** Hash of code deployed on the account itself; null when none. */
  localCodeHash: string | null;
}

export interface ChainReader {
  /** Throws `ViewNotAvailable` when the coin has no such view method. */
  view<T>(method: string, args?: object): Promise<T>;
  account(accountId: string): Promise<AccountFacts>;
  accessKeyCount(accountId: string): Promise<number>;
  /** Events since a block height (default: the coin's launch block) up to the final head. */
  events(since?: number): Promise<{ events: CoinEvent[]; cursor: number }>;
}

/** Reads `?coin=<account>&rpc=<url>` from the page URL. */
export function configFromUrl(href: string): ReaderConfig {
  const params = new URL(href).searchParams;
  const coin = params.get("coin");
  const rpcUrl = params.get("rpc");
  if (!coin) throw new Error("the page URL needs ?coin=<coin account>");
  if (!rpcUrl) throw new Error("the page URL needs &rpc=<NEAR RPC URL>");
  const fastNearUrl = params.get("fastnear") ?? (/testnet/i.test(rpcUrl) ? FASTNEAR_TESTNET_URL : null);
  return fastNearUrl ? { coin, rpcUrl, fastNearUrl } : { coin, rpcUrl };
}

class RpcError extends Error {
  readonly cause: string;
  readonly data: string;
  constructor(cause: string, data: string) {
    super(`${cause}: ${data}`);
    this.cause = cause;
    this.data = data;
  }
  /** The chain has no block at this height (not lost data). */
  get noBlockAtHeight(): boolean {
    return this.cause === "UNKNOWN_BLOCK" && this.data.includes("BLOCK HEIGHT");
  }
}

/** The coin has no view method with this name (a coin built before the view existed). */
export class ViewNotAvailable extends Error {
  readonly method: string;
  constructor(method: string) {
    super(`not available from this coin: ${method}`);
    this.method = method;
  }
}

const NO_LOCAL_CODE = "11111111111111111111111111111111";

const WATCHED = new Set(["fair_chance", "fair_chance_factory"]);

export function chainReader(cfg: ReaderConfig, fetchFn: typeof fetch = fetch): ChainReader {
  async function call<T>(method: string, params: object): Promise<T> {
    const resp = await fetchFn(cfg.rpcUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: "fc", method, params }),
    });
    if (!resp.ok) throw new Error(`${method}: HTTP ${resp.status}`);
    const body = await resp.json();
    if (body.error) throw new RpcError(String(body.error.cause?.name), String(body.error.data));
    return body.result as T;
  }

  async function view<T>(method: string, args: object = {}): Promise<T> {
    let res: { result?: number[]; error?: string };
    try {
      res = await call("query", {
        request_type: "call_function",
        finality: "final",
        account_id: cfg.coin,
        method_name: method,
        args_base64: btoa(JSON.stringify(args)),
      });
    } catch (e) {
      if (e instanceof RpcError && isMethodMissing(`${e.cause} ${e.data}`)) throw new ViewNotAvailable(method);
      throw e;
    }
    if (res.error !== undefined) {
      if (isMethodMissing(res.error)) throw new ViewNotAvailable(method);
      throw new Error(`${method}: ${res.error}`);
    }
    return JSON.parse(new TextDecoder().decode(new Uint8Array(res.result ?? []))) as T;
  }

  async function account(accountId: string): Promise<AccountFacts> {
    const res = await call<{
      code_hash?: string;
      global_contract_hash?: string | null;
      global_contract_account_id?: string | null;
    }>("query", { request_type: "view_account", finality: "final", account_id: accountId });
    return {
      globalContractHash: res.global_contract_hash ?? null,
      usesGlobalContractByAccount: res.global_contract_account_id != null,
      localCodeHash: res.code_hash && res.code_hash !== NO_LOCAL_CODE ? res.code_hash : null,
    };
  }

  async function accessKeyCount(accountId: string): Promise<number> {
    const res = await call<{ keys: unknown[] }>("query", {
      request_type: "view_access_key_list",
      finality: "final",
      account_id: accountId,
    });
    return res.keys.length;
  }

  async function events(since?: number) {
    const from = since ?? (await view<{ launch_block: number }>("fc_rules")).launch_block;
    if (cfg.fastNearUrl) return fastNearEvents({ baseUrl: cfg.fastNearUrl, coin: cfg.coin, since: from, fetchFn });
    // A height with no block answers UNKNOWN_BLOCK and the scan skips it. A node that dropped
    // old blocks answers the same way, so check first that the node still has the blocks.
    const status = await call<{ sync_info: { earliest_block_height: number } }>("status", {});
    const earliest = status.sync_info.earliest_block_height;
    if (from < earliest) {
      throw new Error(
        `the node no longer has the blocks from this cursor: cursor ${from}, earliest block ${earliest}`,
      );
    }
    const head = await call<{ header: { height: number } }>("block", { finality: "final" });
    const out: CoinEvent[] = [];
    const seen = new Set<string>();
    for (let h = from; h <= head.header.height; h++) {
      let block: { header: { timestamp_nanosec: string }; chunks: { chunk_hash: string; height_included: number }[] };
      try {
        block = await call("block", { block_id: h });
      } catch (e) {
        if (e instanceof RpcError && e.noBlockAtHeight) continue;
        throw e;
      }
      for (const ch of block.chunks) {
        if (ch.height_included !== h) continue;
        const chunk = await call<{ transactions: { hash: string; signer_id: string }[] }>("chunk", {
          chunk_id: ch.chunk_hash,
        });
        for (const tx of chunk.transactions) {
          const st = await call<{ receipts_outcome: ReceiptOutcome[] }>("EXPERIMENTAL_tx_status", {
            tx_hash: tx.hash,
            sender_account_id: tx.signer_id,
            wait_until: "FINAL",
          });
          for (const r of st.receipts_outcome) {
            const o = r.outcome;
            if (o.executor_id !== cfg.coin || seen.has(r.id) || !succeeded(o.status)) continue;
            seen.add(r.id);
            for (const line of o.logs) {
              const ev = parseLog(line);
              if (ev) out.push({
                ...ev,
                coin: o.executor_id,
                receiptId: r.id,
                block: h,
                timeNs: block.header.timestamp_nanosec ?? null,
              });
            }
          }
        }
      }
    }
    return { events: out, cursor: head.header.height + 1 };
  }

  return { view, account, accessKeyCount, events };
}

interface ReceiptOutcome {
  id: string;
  outcome: { executor_id: string; logs: string[]; status: Record<string, unknown> };
}

function succeeded(status: Record<string, unknown>): boolean {
  return "SuccessValue" in status || "SuccessReceiptId" in status;
}

function isMethodMissing(text: string): boolean {
  return /MethodNotFound|method .* not found|MethodResolveError/i.test(text);
}

/** One `EVENT_JSON:` log line to the event it carries; null for any other line. */
export function parseLog(line: string): Omit<CoinEvent, "coin" | "receiptId" | "block" | "timeNs"> | null {
  if (!line.startsWith("EVENT_JSON:")) return null;
  try {
    const raw = JSON.parse(line.slice("EVENT_JSON:".length));
    if (!WATCHED.has(raw.standard) || !Array.isArray(raw.data) || raw.data.length === 0) return null;
    return { standard: raw.standard, version: raw.version, name: raw.event, data: raw.data[0] };
  } catch {
    return null;
  }
}
