// Where a plain RPC read of the known testnet RPC goes. On a hosted origin it goes to the page's own
// relay at /api/rpc (web/api/rpc.js), which reads through a keyed provider. On the visitor's own
// machine it goes to the public FastNEAR RPC as before. Any other RPC URL, such as a local sandbox
// from ?rpc=, is never changed. When the relay answers 404 or 503, or fails 3 times in a row, the
// page reads the public RPC for the rest of the page session.
import { KNOWN_NETWORKS } from "./known-deployments.ts";
import { isOwnMachineHost } from "./local-host.ts";

const PUBLIC_RPC = KNOWN_NETWORKS.testnet.rpcUrl;
const FAILURES_BEFORE_FALLBACK = 3;

export interface RpcRoute {
  /** Sends one RPC POST, through the relay when it is on for this URL. */
  post(fetchFn: typeof fetch, rpcUrl: string, init: RequestInit): Promise<Response>;
  /** True while reads of the known testnet RPC go to the relay. */
  relayOn(): boolean;
  /** The host a read of `rpcUrl` goes to now: the relay's host and path, or `rpcHost` itself. */
  readHost(rpcUrl: string, rpcHost: string): string;
}

/** The page's own relay URL; null on the visitor's own machine or outside a browser. */
export function relayUrlFor(origin: string | null): string | null {
  if (origin === null) return null;
  let url: URL;
  try {
    url = new URL(origin);
  } catch {
    return null;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return null;
  if (isOwnMachineHost(url.hostname)) return null;
  return `${url.origin}/api/rpc`;
}

export function createRpcRoute(origin: () => string | null): RpcRoute {
  let relay: string | null | undefined;
  let off = false;
  let failuresInRow = 0;

  const relayUrl = (): string | null => {
    if (relay === undefined) relay = relayUrlFor(origin());
    return off ? null : relay;
  };
  const failed = (): void => {
    failuresInRow += 1;
    if (failuresInRow >= FAILURES_BEFORE_FALLBACK) off = true;
  };

  return {
    relayOn: () => relayUrl() !== null,
    readHost(rpcUrl, rpcHost) {
      const via = rpcUrl === PUBLIC_RPC ? relayUrl() : null;
      if (via === null) return rpcHost;
      const u = new URL(via);
      return `${u.host}${u.pathname}`;
    },
    async post(fetchFn, rpcUrl, init) {
      const via = rpcUrl === PUBLIC_RPC ? relayUrl() : null;
      if (via === null) return fetchFn(rpcUrl, init);
      let resp: Response;
      try {
        resp = await fetchFn(via, init);
      } catch (e) {
        failed();
        throw e;
      }
      // No relay on this deploy, or no provider URL set: read the public RPC from now on.
      if (resp.status === 404 || resp.status === 503) {
        off = true;
        return fetchFn(rpcUrl, init);
      }
      // A read the relay does not allow goes to the public RPC; the relay stays on for the others.
      if (resp.status === 403) return fetchFn(rpcUrl, init);
      if (resp.status === 429 || resp.status >= 500) failed();
      else failuresInRow = 0;
      return resp;
    },
  };
}

/** The one route of the page, from the page's own origin. */
export const pageRpcRoute = createRpcRoute(() => (typeof location === "undefined" ? null : location.origin));
