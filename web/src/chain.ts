// Chain-read module: views and events for one coin, from a NEAR RPC endpoint.
// The page reads the chain only through this module, never through the watcher.

export interface ReaderConfig {
  rpcUrl: string;
  coin: string;
}

export interface CoinEvent {
  standard: "fair_chance" | "fair_chance_factory";
  version: string;
  name: string;
  coin: string;
  receiptId: string;
  /** Height of the block that included the transaction. */
  block: number;
  data: Record<string, unknown>;
}

export interface ChainReader {
  view<T>(method: string, args?: object): Promise<T>;
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
  return { coin, rpcUrl };
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
    const res = await call<{ result: number[] }>("query", {
      request_type: "call_function",
      finality: "final",
      account_id: cfg.coin,
      method_name: method,
      args_base64: btoa(JSON.stringify(args)),
    });
    return JSON.parse(new TextDecoder().decode(new Uint8Array(res.result))) as T;
  }

  async function events(since?: number) {
    const from = since ?? (await view<{ launch_block: number }>("fc_rules")).launch_block;
    const head = await call<{ header: { height: number } }>("block", { finality: "final" });
    const out: CoinEvent[] = [];
    const seen = new Set<string>();
    for (let h = from; h <= head.header.height; h++) {
      let block: { chunks: { chunk_hash: string; height_included: number }[] };
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
              if (ev) out.push({ ...ev, coin: o.executor_id, receiptId: r.id, block: h });
            }
          }
        }
      }
    }
    return { events: out, cursor: head.header.height + 1 };
  }

  return { view, events };
}

interface ReceiptOutcome {
  id: string;
  outcome: { executor_id: string; logs: string[]; status: Record<string, unknown> };
}

function succeeded(status: Record<string, unknown>): boolean {
  return "SuccessValue" in status || "SuccessReceiptId" in status;
}

function parseLog(line: string): Omit<CoinEvent, "coin" | "receiptId" | "block"> | null {
  if (!line.startsWith("EVENT_JSON:")) return null;
  try {
    const raw = JSON.parse(line.slice("EVENT_JSON:".length));
    if (!WATCHED.has(raw.standard) || !Array.isArray(raw.data) || raw.data.length === 0) return null;
    return { standard: raw.standard, version: raw.version, name: raw.event, data: raw.data[0] };
  } catch {
    return null;
  }
}
