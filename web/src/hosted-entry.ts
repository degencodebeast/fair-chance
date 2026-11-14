// What a hosted page shows before any screen: the fixed network banner, and the link a bare hosted
// address opens. Both come from the fixed config, never from the link.
import { el } from "./dom-elements.ts";
import { isOwnMachineHost } from "./local-host.ts";

export const TESTNET_BANNER = "NEAR testnet. Test tokens have no value. No audit.";

/** The fixed banner text for a link that names the testnet; null for any other link. */
export function networkBanner(href: string): string | null {
  return new URL(href).searchParams.get("network") === "testnet" ? TESTNET_BANNER : null;
}

/**
 * The link a bare hosted address gets: it names the testnet and nothing else, so the landing screen
 * opens and every link from it carries the network. Null when the link has any parameter or the host is local.
 */
export function hostedEntryHref(href: string): string | null {
  const url = new URL(href);
  if (isOwnMachineHost(url.hostname) || url.search !== "") return null;
  return `?${new URLSearchParams({ network: "testnet" }).toString()}`;
}

/** Puts the fixed banner before the app, once. Draws nothing when the link names no known network. */
export function showNetworkBanner(doc: Document, href: string): void {
  const text = networkBanner(href);
  const app = doc.querySelector("#app");
  if (text === null || app === null || doc.querySelector("#network-banner") !== null) return;
  app.before(el("div", { id: "network-banner", role: "note" }, text));
}
