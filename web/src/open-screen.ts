// Browser entry: opens the screen the page link names. The coin page draws itself when its module
// loads; the other screens are drawn here.
import "./screens.css";
import { screenFor } from "./screen-route.ts";
import { hostedEntryHref, showNetworkBanner } from "./hosted-entry.ts";

const app = document.querySelector<HTMLElement>("#app")!;
// A bare hosted address opens on the demo coin from the fixed config.
const entry = hostedEntryHref(location.href);
if (entry !== null) history.replaceState(null, "", entry);
const href = location.href;
showNetworkBanner(document, href);
const screen = screenFor(href);

const TITLES = { landing: "Fair Chance", coins: "Fair Chance coins", alerts: "Fair Chance alerts", launch: "Launch a coin", coin: "Fair Chance coin" };
document.title = TITLES[screen];

async function open(): Promise<void> {
  if (screen === "coin") {
    await import("./render-coin-page.ts");
  } else if (screen === "coins") {
    (await import("./render-coin-list.ts")).drawCoinList(app, href);
  } else if (screen === "launch") {
    (await import("./render-launch-form.ts")).drawLaunchForm(app, href);
  } else if (screen === "alerts") {
    (await import("./render-alerts-screen.ts")).drawAlertsScreen(app, href);
  } else {
    (await import("./render-landing.ts")).drawLanding(app, href);
  }
}

void open();
