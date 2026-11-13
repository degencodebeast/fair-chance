// What a hosted page shows before any screen: the fixed network banner, and the link a bare hosted
// address opens. Both come from the fixed config, never from the link.
import { el } from "./dom-elements.ts";
import { KNOWN_NETWORKS, type KnownNetwork } from "./known-deployments.ts";

export const TESTNET_BANNER = "Testnet. Read only. No wallet needed.";

/** The fixed banner text for a link that names the testnet; null for any other link. */
export function networkBanner(href: string): string | null {
  return new URL(href).searchParams.get("network") === "testnet" ? TESTNET_BANNER : null;
}

function isLocalHost(hostname: string): boolean {
  return hostname === "localhost" || hostname === "127.0.0.1";
}

/**
 * The link a bare hosted address redirects to: the demo coin of the testnet. Null when the link has
 * any parameter, the host is local, or the config has no demo coin yet.
 */
export function hostedEntryHref(href: string, networks: Record<string, KnownNetwork> = KNOWN_NETWORKS): string | null {
  const url = new URL(href);
  if (isLocalHost(url.hostname) || url.search !== "") return null;
  const demo = networks.testnet?.demoCoin ?? null;
  return demo ? `?${new URLSearchParams({ network: "testnet", coin: demo }).toString()}` : null;
}

/** Puts the fixed banner before the app, once. Draws nothing when the link names no known network. */
export function showNetworkBanner(doc: Document, href: string): void {
  const text = networkBanner(href);
  const app = doc.querySelector("#app");
  if (text === null || app === null || doc.querySelector("#network-banner") !== null) return;
  app.before(el("div", { id: "network-banner", role: "note" }, text));
}
