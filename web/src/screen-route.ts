// Which screen a page link opens, and the links between screens. A link between screens keeps only
// the parameters that pick where the page reads from; the page-link rules check them where they are used.
export type Screen = "landing" | "coins" | "alerts" | "launch" | "mine" | "coin";

const CARRIED = ["network", "rpc", "factory", "code_hash", "fastnear", "from_block"] as const;

export function screenFor(href: string): Screen {
  const params = new URL(href).searchParams;
  const view = params.get("view");
  // The alerts screen may name a coin: its "Open on Fair Chance" links go to that coin.
  if (view === "alerts") return "alerts";
  if (view === "launch") return "launch";
  if (view === "mine") return "mine";
  if (params.get("coin")) return "coin";
  return view === "coins" ? view : "landing";
}

export function screenLink(from: string, screen: Screen, extra: { coin?: string; receipt?: string } = {}): string {
  const source = new URL(from).searchParams;
  const out = new URLSearchParams();
  if (screen === "coins" || screen === "alerts" || screen === "launch" || screen === "mine") out.set("view", screen);
  if (screen === "coin") {
    out.set("coin", extra.coin ?? "");
    if (extra.receipt) out.set("receipt", extra.receipt);
  }
  for (const name of CARRIED) {
    const value = source.get(name);
    if (value !== null) out.set(name, value);
  }
  return `?${out.toString()}`;
}
