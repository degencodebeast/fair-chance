// Turns the page link into what the page reads and trusts. The link picks a network by name
// (`?network=testnet`). The RPC URL, the FastNEAR URL, the factory and the coin code hash then come
// from the fixed list in known-deployments.ts. The one exception is a local sandbox: when the link's
// RPC host is exactly `localhost` or `127.0.0.1` (exact match, no suffix or label match), the link
// may give the RPC URL, the factory and the code hash. A hostile RPC could otherwise answer every
// chain check with false facts.
import { isNetworkName, KNOWN_NETWORKS, type KnownDeployment } from "./known-deployments.ts";

export interface PageLink {
  coin: string;
  rpcUrl: string;
  /** Host and port of `rpcUrl`, the host the checks list names. */
  rpcHost: string;
  fastNearUrl?: string;
  known: KnownDeployment;
}

function localSandboxUrl(text: string | null): URL | null {
  if (text === null) return null;
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    return null;
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return null;
  return url.hostname === "localhost" || url.hostname === "127.0.0.1" ? url : null;
}

export function resolvePageLink(href: string): PageLink {
  const params = new URL(href).searchParams;
  const coin = params.get("coin");
  if (!coin) throw new Error("the page URL needs ?coin=<coin account>");
  const network = params.get("network");
  if (network !== null) {
    if (!isNetworkName(network)) throw new Error(`the page does not know the network "${network}"; use ?network=testnet`);
    const fixed = KNOWN_NETWORKS[network];
    return {
      coin,
      rpcUrl: fixed.rpcUrl,
      rpcHost: new URL(fixed.rpcUrl).host,
      fastNearUrl: fixed.fastNearUrl,
      known: { factory: fixed.factory, codeHash: fixed.codeHash },
    };
  }
  const local = localSandboxUrl(params.get("rpc"));
  if (local === null) {
    throw new Error("the page URL needs &network=testnet (a link may give &rpc=<URL> only for a local sandbox)");
  }
  const rpcUrl = params.get("rpc")!;
  // The FastNEAR URL follows the same rule as the RPC URL: a link may give it only for a local host.
  const fastNear = params.get("fastnear");
  const fastNearUrl = localSandboxUrl(fastNear) !== null ? fastNear : null;
  return {
    coin,
    rpcUrl,
    rpcHost: local.host,
    ...(fastNearUrl ? { fastNearUrl } : {}),
    known: { factory: params.get("factory"), codeHash: params.get("code_hash") },
  };
}

/** Names where the page reads from, for the error text: "NEAR testnet", "the local sandbox", else "the chain". */
export function sourceLabel(href: string): string {
  try {
    const params = new URL(href).searchParams;
    const network = params.get("network");
    if (network !== null) return isNetworkName(network) ? `NEAR ${network}` : "the chain";
    return localSandboxUrl(params.get("rpc")) !== null ? "the local sandbox" : "the chain";
  } catch {
    return "the chain";
  }
}

/** What the coin list reads: nothing (sample data), or the factory's launch events. */
export type ListLink =
  | { kind: "sample" }
  | { kind: "no-factory"; source: string }
  | { kind: "chain"; rpcUrl: string; rpcHost: string; fastNearUrl?: string; factory: string; fromBlock: number; source: string };

/** The coin list follows the coin page rules: a link picks only the network, except a local sandbox. */
export function resolveListLink(href: string): ListLink {
  const params = new URL(href).searchParams;
  const network = params.get("network");
  if (network !== null) {
    if (!isNetworkName(network)) throw new Error(`the page does not know the network "${network}"; use ?network=testnet`);
    const fixed = KNOWN_NETWORKS[network];
    const source = `NEAR ${network}`;
    if (fixed.factory === null) return { kind: "no-factory", source };
    return { kind: "chain", rpcUrl: fixed.rpcUrl, rpcHost: new URL(fixed.rpcUrl).host, fastNearUrl: fixed.fastNearUrl, factory: fixed.factory, fromBlock: fixed.factoryFromBlock, source };
  }
  if (params.get("rpc") === null) return { kind: "sample" };
  const local = localSandboxUrl(params.get("rpc"));
  if (local === null) {
    throw new Error("the page URL needs &network=testnet (a link may give &rpc=<URL> only for a local sandbox)");
  }
  const source = "the local sandbox";
  const factory = params.get("factory");
  if (!factory) return { kind: "no-factory", source };
  const fastNear = params.get("fastnear");
  const from = Number(params.get("from_block") ?? "0");
  return {
    kind: "chain",
    rpcUrl: params.get("rpc")!,
    rpcHost: local.host,
    ...(localSandboxUrl(fastNear) !== null && fastNear ? { fastNearUrl: fastNear } : {}),
    factory,
    fromBlock: Number.isSafeInteger(from) && from >= 0 ? from : 0,
    source,
  };
}
