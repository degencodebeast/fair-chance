// Browser entry: opens the screen the page link names. The coin page draws itself when its module
// loads; the other screens are drawn here.
import "./screens.css";
import { screenFor } from "./screen-route.ts";
import { hostedEntryHref, showNetworkBanner } from "./hosted-entry.ts";
import { mountWalletBar, pageWallet } from "./page-wallet.ts";

const app = document.querySelector<HTMLElement>("#app")!;
// A bare hosted address opens the landing screen on the testnet.
const entry = hostedEntryHref(location.href);
if (entry !== null) history.replaceState(null, "", entry);
const href = location.href;
showNetworkBanner(document, href);
// The wallet bar sits on every screen, outside the part each screen redraws.
mountWalletBar(document, pageWallet(href));
const screen = screenFor(href);

const TITLES = { landing: "Fair Chance", coins: "Fair Chance coins", alerts: "Fair Chance alerts", launch: "Launch a coin", mine: "My coins", coin: "Fair Chance coin" };
document.title = TITLES[screen];

async function open(): Promise<void> {
  if (screen === "coin") {
    await import("./render-coin-page.ts");
    // The recorded run sits after the app, so a redraw of the live coin page never removes it.
    const run = await (await import("./recorded-run.ts")).loadRecordedRun(href);
    if (run !== null) app.after((await import("./render-recorded-run.ts")).drawRecordedRun(run));
  } else if (screen === "coins") {
    (await import("./render-coin-list.ts")).drawCoinList(app, href);
  } else if (screen === "launch") {
    const [{ drawLaunchForm }, { sendLaunch }, { saveLaunched }, { screenLink }] = await Promise.all([
      import("./render-launch-form.ts"),
      import("./trade-actions.ts"),
      import("./launch-handoff.ts"),
      import("./screen-route.ts"),
    ]);
    drawLaunchForm(app, href, undefined, undefined, {
      wallet: pageWallet(href),
      launch: sendLaunch,
      // The new coin's page shows it at once from the launch result, then reads the chain.
      onLaunched(coin) {
        saveLaunched(sessionStorage, coin);
        location.assign(screenLink(href, "coin", { coin: coin.coin }));
      },
    });
  } else if (screen === "mine") {
    const [{ drawMyCoins }, { myCoinsFromLink }] = await Promise.all([import("./render-my-coins.ts"), import("./my-coins.ts")]);
    drawMyCoins(app, href, { wallet: pageWallet(href), load: myCoinsFromLink(href) });
  } else if (screen === "alerts") {
    (await import("./render-alerts-screen.ts")).drawAlertsScreen(app, href);
  } else {
    (await import("./render-landing.ts")).drawLanding(app, href);
  }
}

void open();
