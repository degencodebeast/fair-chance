// The My coins screen: the coins the signed-in account launched and the coins it holds, each with
// what it can sell now. It reads again on each sign-in. Read-only: nothing here signs.
import { drawCoinTile } from "./coin-tile.ts";
import { el } from "./dom-elements.ts";
import { compactTokens } from "./format.ts";
import type { MyCoin, MyCoins } from "./my-coins.ts";
import { screenHeader } from "./screen-chrome.ts";
import { screenLink } from "./screen-route.ts";
import type { WalletControl } from "./wallet-session.ts";

export interface MyCoinsDeps {
  wallet: WalletControl;
  load(accountId: string): Promise<MyCoins>;
}

function item(c: MyCoin, href: string): HTMLElement {
  const t = (units: bigint) => `${compactTokens(units.toString(), c.decimals)} ${c.ticker}`;
  const holding = c.balance === 0n ? `You hold no ${c.ticker}.` : `You hold ${t(c.balance)}. You can sell up to ${t(c.canSellNow)} now.`;
  return el(
    "li",
    { class: "mine-coin card" },
    el(
      "a",
      { class: "mine-coin-link", href: screenLink(href, "coin", { coin: c.coin }) },
      drawCoinTile({ account: c.coin, ticker: c.ticker, icon: c.icon, small: true }),
      el("span", {}, el("b", {}, c.name), " ", el("span", { class: "num" }, c.ticker)),
    ),
    el("p", { class: "num muted" }, c.coin),
    el("p", {}, holding),
  );
}

function list(id: string, coins: MyCoin[], href: string, empty: string): HTMLElement {
  return coins.length === 0 ? el("p", { id, class: "muted" }, empty) : el("ul", { id, class: "mine-list" }, ...coins.map((c) => item(c, href)));
}

export function drawMyCoins(app: HTMLElement, href: string, deps: MyCoinsDeps): void {
  const status = el("p", { id: "mine-status", class: "status", role: "status" });
  const body = el("div", { id: "mine-body" });
  app.replaceChildren(screenHeader(href, "mine"), el("h1", { id: "mine-title" }, "My coins"), status, body);

  let latest = 0;
  const show = (): void => {
    const mine = ++latest;
    const s = deps.wallet.state();
    body.replaceChildren();
    if (s.kind !== "signed-in") {
      status.textContent = "Connect a wallet to see the coins you launched and hold.";
      return;
    }
    const account = s.session.accountId;
    status.textContent = `Reading the coins of ${account}.`;
    deps.load(account).then(
      (coins) => {
        if (mine !== latest) return;
        status.textContent = `Coins of ${account}.`;
        body.replaceChildren(
          el("h2", {}, "Coins you launched"),
          list("mine-launched", coins.launched, href, "You launched no coin yet."),
          el("h2", {}, "Coins you hold"),
          list("mine-held", coins.held, href, "You hold no coin from this factory."),
        );
      },
      (e: unknown) => {
        if (mine === latest) status.textContent = `Your coins cannot load from the chain. ${e instanceof Error ? e.message : String(e)}`;
      },
    );
  };
  const stop = deps.wallet.subscribe(() => (app.contains(status) ? show() : stop()));
  show();
}
