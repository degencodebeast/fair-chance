// Checks whether a coin account name is free, with one read-only view_account call through the
// RPC the page link fixes. It never signs. Only an answer that says the account does not exist
// gives "free"; any other failure is thrown, so the form cannot show "free" by mistake.
import { readWithRetry, type RetryOptions } from "./chain-fetch.ts";
import { resolveListLink } from "./page-link.ts";
import { httpStatusError } from "./refresher.ts";
import { pageRpcRoute } from "./rpc-route.ts";

export type NameState = "free" | "taken";

export async function checkAccountName(cfg: { rpcUrl: string; accountId: string }, fetchFn: typeof fetch = fetch, retry: RetryOptions = {}): Promise<NameState> {
  const body = await readWithRetry(async () => {
    const resp = await pageRpcRoute.post(fetchFn, cfg.rpcUrl, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: "fc", method: "query", params: { request_type: "view_account", finality: "final", account_id: cfg.accountId } }),
    });
    if (!resp.ok) throw httpStatusError(`view_account: HTTP ${resp.status}`, resp);
    return resp.json();
  }, retry);
  if (body.error) {
    if (body.error.cause?.name === "UNKNOWN_ACCOUNT") return "free";
    throw new Error(`view_account: ${String(body.error.cause?.name)}: ${String(body.error.data)}`);
  }
  const result = body.result;
  if (typeof result?.error === "string") {
    if (/does not exist/i.test(result.error)) return "free";
    throw new Error(`view_account: ${result.error}`);
  }
  if (typeof result?.code_hash === "string") return "taken";
  throw new Error("view_account: the answer has no account facts");
}

export interface LaunchNameCheck {
  /** The factory the coin account would sit under; null when the link names none. */
  factory: string | null;
  /** Null when nothing can be checked from this link. */
  checkName: ((accountId: string) => Promise<NameState>) | null;
}

/** The factory and RPC come from the same fixed rules as the coin list (page-link.ts). */
export function launchNameCheckFromLink(href: string): LaunchNameCheck {
  try {
    const link = resolveListLink(href);
    if (link.kind !== "chain") return { factory: null, checkName: null };
    return { factory: link.factory, checkName: (accountId) => checkAccountName({ rpcUrl: link.rpcUrl, accountId }) };
  } catch {
    return { factory: null, checkName: null };
  }
}
