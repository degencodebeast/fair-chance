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
  const fastNearUrl = params.get("fastnear");
  return {
    coin,
    rpcUrl,
    rpcHost: local.host,
    ...(fastNearUrl ? { fastNearUrl } : {}),
    known: { factory: params.get("factory"), codeHash: params.get("code_hash") },
  };
}
