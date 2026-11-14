// Opens the built landing, coin list, alerts and coin screens in headless Google Chrome at widths
// 1440 and 390. Every outside host is blocked and the chain is a stub that answers from the recorded
// coin page state, so no request leaves the machine. It checks, for each screen:
//   - the document is no wider than the window
//   - every value not read from chain says "Sample data" on its own card
//   - no control looks live when it does nothing
//   - the coin list reads the factory's launch events, and the coin page draws the price chart
//     and the holders table from the coin's events and fc_account views
//   node scripts/check-screens-in-browser.ts
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium, type Browser, type Page } from "playwright-core";
import { buildAndServePage } from "./page-server.ts";

const testdata = (path: string) => JSON.parse(readFileSync(join(fileURLToPath(new URL("../../testdata/", import.meta.url)), path), "utf8"));
const state = testdata("coin-page-states/normal.json");
const holderView = testdata("account-plan-states/cap-full-floor.json").views.fc_account;

const RPC = "http://127.0.0.1:3030";
const FASTNEAR = "http://127.0.0.1:3031";
const FACTORY = "factory.sandbox";
const COIN = "moth.factory.sandbox";
const widths = [1440, 390];
const NEAR = "1000000000000000000000000";

const swap = (name: string, block: number, data: object) => ({
  receipt_id: `Swap${block}`, block, time_ns: `${1794578000 + block}000000000`,
  line: `EVENT_JSON:${JSON.stringify({ standard: "fair_chance", version: "1.0.0-rc.5", event: name, data: [data] })}`,
});
const coinEvents = [
  ...state.events,
  swap("fc_buy", 2101, { account: "alice.testnet", near_in: NEAR, tokens_out: `5000000${"0".repeat(18)}`, marked_tokens: "0", class_after: "small", block: 2101 }),
  swap("fc_buy", 2102, { account: "bob.testnet", near_in: NEAR, tokens_out: `4000000${"0".repeat(18)}`, marked_tokens: "0", class_after: "small", block: 2102 }),
];
const launchLine = `EVENT_JSON:${JSON.stringify({ standard: "fair_chance_factory", version: "1.0.0-rc.5", event: "fc_launch", data: [{ coin: COIN, creator: "lamp.testnet", code_hash: "x", launch_block: 100, description: null }] })}`;

interface Stub { holdersFail: boolean }

async function openScreen(browser: Browser, width: number, query: string, stub: Stub = { holdersFail: false }, seen: string[] = []): Promise<Page> {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "*" };
  page.on("request", (r) => seen.push(r.url()));
  // Anything outside the page's own host is blocked; fonts are not reads and are ignored below.
  await page.route(/^https?:\/\/(?!127\.0\.0\.1)/, (route) => route.abort());
  await page.route(`${RPC}/**`, async (route) => {
    if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
    const q = JSON.parse(route.request().postData() ?? "{}").params ?? {};
    let answer: object = {};
    if (q.request_type === "call_function") {
      let view = state.views[q.method_name as string];
      if (q.method_name === "fc_account") {
        if (stub.holdersFail) return route.fulfill({ status: 503, headers: cors, body: "unavailable" });
        view = holderView;
      }
      answer = { result: { result: Array.from(new TextEncoder().encode(JSON.stringify(view))) } };
    } else if (q.request_type === "view_account") answer = { result: state.account };
    else if (q.request_type === "view_access_key_list") answer = { result: { keys: [] } };
    return route.fulfill({ status: 200, contentType: "application/json", headers: cors, body: JSON.stringify({ jsonrpc: "2.0", id: "fc", ...answer }) });
  });
  await page.route(`${FASTNEAR}/**`, async (route) => {
    if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
    const path = new URL(route.request().url()).pathname;
    const body = JSON.parse(route.request().postData() ?? "{}");
    const lines: { hash: string; account: string; block: number; ts: string; line: string }[] = [
      { hash: "TxLaunch", account: FACTORY, block: 100, ts: "1794574920000000000", line: launchLine },
      ...coinEvents.map((e, i) => ({ hash: `TxCoin${i}`, account: COIN, block: e.block, ts: e.time_ns, line: e.line })),
    ];
    let answer: object;
    if (path === "/v0/account") {
      answer = { account_txs: lines.filter((l) => l.account === body.account_id).map((l) => ({ transaction_hash: l.hash, tx_block_height: l.block, tx_block_timestamp: l.ts })), resume_token: null };
    } else {
      answer = {
        transactions: lines.filter((l) => (body.tx_hashes as string[]).includes(l.hash)).map((l) => ({
          transaction: { hash: l.hash },
          receipts: [{ receipt: { receipt_id: `R${l.hash}`, receiver_id: l.account }, execution_outcome: { outcome: { logs: [l.line], status: { SuccessValue: "" } } } }],
        })),
      };
    }
    return route.fulfill({ status: 200, contentType: "application/json", headers: cors, body: JSON.stringify(answer) });
  });
  await page.goto(`${served.origin}/${query}`);
  return page;
}

function fail(label: string, what: string): never {
  throw new Error(`${label}: ${what}`);
}
async function noOverflow(page: Page, width: number, label: string): Promise<void> {
  const w = await page.evaluate(() => document.documentElement.scrollWidth);
  if (w > width) fail(label, `the document is ${w} px wide`);
}
function readsOutsideFonts(seen: string[]): string[] {
  return seen.filter((u) => !u.startsWith("http://127.0.0.1") && !/fonts\.(googleapis|gstatic)\.com/.test(u) && !u.startsWith("data:"));
}

const served = await buildAndServePage();
let browser: Browser | undefined;
let exitCode = 0;
try {
  const executablePath = process.env.FC_BROWSER_PATH || undefined;
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : { channel: "chrome" }) });

  for (const width of widths) {
    // Landing
    let label = `landing at ${width}`;
    let seen: string[] = [];
    let page = await openScreen(browser, width, "", undefined, seen);
    await page.waitForSelector("#landing-headline");
    const landing = await page.evaluate(() => ({
      headline: document.querySelector("#landing-headline")?.textContent,
      pitch: document.querySelector("#landing-headline + .hero-sub")?.textContent,
      openApp: document.querySelector<HTMLAnchorElement>("#open-app")?.getAttribute("href"),
      launchOff: document.querySelector<HTMLButtonElement>("#launch-coin")?.disabled,
      launchReason: document.querySelector("#launch-coin-reason")?.textContent?.trim(),
      proofSample: document.querySelector("#proof .sample-label")?.textContent,
      buttonsThatDoNothing: [...document.querySelectorAll("button:not([disabled])")].map((b) => b.textContent),
    }));
    if (landing.headline !== "You buy. The cabal dumps on you. You're left holding the bag.") fail(label, `headline is "${landing.headline}"`);
    if (landing.pitch !== "Fair Chance slows the dump at launch, inside the coin contract.") fail(label, `the line under the headline is "${landing.pitch}"`);
    if (!/view=coins/.test(landing.openApp ?? "")) fail(label, `Open app goes to ${landing.openApp}`);
    if (landing.launchOff !== true || landing.launchReason !== "Launching signs a transaction. This page does not sign yet.") fail(label, `Launch a coin must be turned off with its reason: ${landing.launchReason}`);
    if (!/^Sample data/.test(landing.proofSample ?? "")) fail(label, "the example receipt has no sample label");
    if (landing.buttonsThatDoNothing.length > 0) fail(label, `buttons that do nothing: ${landing.buttonsThatDoNothing.join(", ")}`);
    await noOverflow(page, width, label);
    if (readsOutsideFonts(seen).length > 0) fail(label, `outside requests: ${readsOutsideFonts(seen).join(", ")}`);
    await page.close();

    // Coin list, sample (no factory): every card says Sample data on itself
    for (const query of ["?view=coins", "?view=coins&network=testnet"]) {
      label = `coin list ${query} at ${width}`;
      seen = [];
      page = await openScreen(browser, width, query, undefined, seen);
      await page.waitForSelector(".coin-card");
      const list = await page.evaluate(() => ({
        cards: [...document.querySelectorAll(".coin-card")].map((c) => ({ sample: c.querySelector(".sample-label")?.textContent ?? "", interactive: c.matches("a, button") || c.querySelector("a, button") !== null })),
        note: document.querySelector("#list-note")?.textContent ?? "",
        launchOff: document.querySelector<HTMLButtonElement>("#launch-coin")?.disabled,
        launchReason: document.querySelector("#launch-coin-reason")?.textContent?.trim(),
      }));
      if (list.cards.length < 5) fail(label, `${list.cards.length} cards`);
      if (list.launchOff !== true || list.launchReason !== "Launching signs a transaction. This page does not sign yet.") fail(label, `Launch a coin must be turned off with its reason: ${list.launchReason}`);
      if (!list.cards.every((c) => c.sample.startsWith("Sample data"))) fail(label, "a card has no sample label on itself");
      if (list.cards.some((c) => c.interactive)) fail(label, "a sample card looks openable");
      if (query.includes("network") && !/no factory/.test(list.note)) fail(label, `the note does not say there is no factory: "${list.note}"`);
      await page.fill("#coin-search", "xyz");
      await page.waitForSelector("#list-empty");
      const empty = await page.textContent("#list-empty");
      if (empty?.trim() !== 'No coin matches "xyz".') fail(label, `empty text is "${empty}"`);
      await noOverflow(page, width, label);
      if (readsOutsideFonts(seen).length > 0) fail(label, `outside requests: ${readsOutsideFonts(seen).join(", ")}`);
      await page.close();
    }

    // Coin list read from the factory
    label = `coin list from the factory at ${width}`;
    page = await openScreen(browser, width, `?view=coins&rpc=${encodeURIComponent(RPC)}&fastnear=${encodeURIComponent(FASTNEAR)}&factory=${FACTORY}`);
    await page.waitForSelector(".coin-card", { timeout: 60_000 });
    const chain = await page.evaluate(() => ({
      count: document.querySelectorAll(".coin-card").length,
      sample: document.querySelectorAll(".sample-label").length,
      text: document.querySelector(".coin-card")?.textContent ?? "",
      href: document.querySelector<HTMLAnchorElement>("a.coin-card")?.getAttribute("href") ?? "",
    }));
    if (chain.count !== 1 || chain.sample !== 0) fail(label, `cards ${chain.count}, sample labels ${chain.sample}`);
    if (!/Moth Lamp/.test(chain.text) || !/172\.99 NEAR/.test(chain.text)) fail(label, `the card does not show the chain values: ${chain.text}`);
    if (!new URLSearchParams(chain.href).has("coin", COIN)) fail(label, `the card opens ${chain.href}`);
    await noOverflow(page, width, label);
    await page.close();

    // Alerts
    label = `alerts at ${width}`;
    page = await openScreen(browser, width, "?view=alerts");
    await page.waitForSelector(".chat-bubble");
    const alerts = await page.evaluate(() => ({
      bubbles: document.querySelectorAll(".chat-bubble").length,
      label: document.querySelector("#alerts-sample-label")?.textContent ?? "",
      preBuy: document.querySelector("#pre-buy-card .sample-label")?.textContent ?? "",
      cardLabels: ["section[aria-label='Telegram chat']", "section[aria-label='Plain text']", "#pre-buy-card"].map((sel) => document.querySelector(`${sel} .sample-label`)?.textContent ?? ""),
      headings: document.querySelectorAll(".chat-head").length,
      opens: [...document.querySelectorAll(".open-on-fair-chance")].map((o) => ({ tag: o.tagName, off: (o as HTMLButtonElement).disabled === true })),
    }));
    if (alerts.cardLabels.some((t) => !/^Sample data/.test(t))) fail(label, `a sample card has no label of its own: ${JSON.stringify(alerts.cardLabels)}`);
    if (alerts.headings > 0) fail(label, "a bubble shows a heading the bot does not send");
    if (alerts.bubbles < 10) fail(label, `${alerts.bubbles} chat bubbles`);
    if (!/^Sample data/.test(alerts.label) || !/^Sample data/.test(alerts.preBuy)) fail(label, "a sample label is missing");
    if (alerts.opens.length === 0 || alerts.opens.some((o) => o.tag !== "BUTTON" || !o.off)) fail(label, "Open on Fair Chance must be turned off with no coin");
    await noOverflow(page, width, label);
    await page.close();

    label = `alerts with a coin in the link at ${width}`;
    page = await openScreen(browser, width, `?view=alerts&coin=${COIN}&rpc=${encodeURIComponent(RPC)}`);
    await page.waitForSelector(".chat-bubble");
    const live = await page.evaluate(() => ({
      links: document.querySelectorAll("a.open-on-fair-chance").length,
      off: [...document.querySelectorAll("button.open-on-fair-chance")].every((b) => (b as HTMLButtonElement).disabled),
      buttons: document.querySelectorAll("button.open-on-fair-chance").length,
    }));
    if (live.links !== 0 || live.buttons === 0 || !live.off) fail(label, `sample alerts must have no live link: ${JSON.stringify(live)}`);
    await noOverflow(page, width, label);
    await page.close();

    // Coin page: price chart and holders table
    label = `coin page chart and holders at ${width}`;
    page = await openScreen(browser, width, `?coin=${COIN}&rpc=${encodeURIComponent(RPC)}&fastnear=${encodeURIComponent(FASTNEAR)}&factory=${FACTORY}`);
    await page.waitForSelector("#holders .holder-row", { timeout: 60_000 });
    const coinPage = await page.evaluate(() => ({
      chartLine: document.querySelector("#price-chart svg polyline")?.getAttribute("points") ?? "",
      caption: document.querySelector("#price-chart .chart-caption")?.textContent ?? "",
      rows: [...document.querySelectorAll("#holders .holder-row")].map((r) => r.textContent ?? ""),
      sample: document.querySelectorAll("#price-chart .sample-label, #holders .sample-label").length,
    }));
    if (coinPage.chartLine.split(" ").length < 3) fail(label, `chart points: ${coinPage.chartLine}`);
    if (!/trade/i.test(coinPage.caption)) fail(label, `chart caption: ${coinPage.caption}`);
    if (!coinPage.rows.some((r) => /Large/.test(r) && /30\.0M/.test(r))) fail(label, `holder rows: ${JSON.stringify(coinPage.rows)}`);
    if (coinPage.sample !== 0) fail(label, "chain data must not carry a sample label");
    await noOverflow(page, width, label);
    await page.close();

    label = `coin page when the holders cannot be read at ${width}`;
    page = await openScreen(browser, width, `?coin=${COIN}&rpc=${encodeURIComponent(RPC)}&fastnear=${encodeURIComponent(FASTNEAR)}&factory=${FACTORY}`, { holdersFail: true });
    await page.waitForSelector("#holders-error", { timeout: 60_000 });
    if ((await page.locator("#price-chart svg polyline").count()) !== 1) fail(label, "the chart must still draw");
    await noOverflow(page, width, label);
    await page.close();
  }
  console.log("screens ok: landing, coin list, alerts and coin page chart and holders at 1440 and 390");
} catch (err) {
  console.error(`check-screens-in-browser: ${err instanceof Error ? err.message : String(err)}`);
  exitCode = 1;
} finally {
  await browser?.close();
  await served.close();
}
process.exit(exitCode);
