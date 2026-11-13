// Opens the coin page and the launch form in headless Google Chrome at widths 1440 and 390, with
// every outside host blocked and the chain answered from a stub. It checks:
//   - the coin page is a two-column grid at 1440 (who | plan, chart | trade box, receipts | trade box,
//     holders, rules) and follows the v5 phone order at 390; the document is no wider than the window
//   - every control that is turned on has a handler, and every turned-off button points at a reason
//   - connect, buy, sell, claim, launch and the picture button are turned off, and the buy reason
//     says a buy from an account with no storage record also pays the storage deposit
//   - the large-buy warning shows when the stubbed fc_quote_buy says becomes_large, and not when it does not
//   - the launch description field has the launch form's length limit
//   node scripts/check-turned-off-screens-in-browser.ts
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium, type Browser, type Page } from "playwright-core";
import { LAUNCH_LIMITS } from "../src/launch-form-model.ts";
import { buildAndServePage } from "./page-server.ts";

const testdata = (path: string) => JSON.parse(readFileSync(join(fileURLToPath(new URL("../../testdata/", import.meta.url)), path), "utf8"));
const state = testdata("coin-page-states/normal.json");
const holderView = testdata("account-plan-states/cap-full-floor.json").views.fc_account;

const RPC = "http://127.0.0.1:3030";
const FASTNEAR = "http://127.0.0.1:3031";
const COIN = "moth.factory.sandbox";
const widths = [1440, 390];
const HOUR_NS = 3_600_000_000_000n;
const creatorFees = {
  creator: "lamp.testnet",
  balance: "420000000000000000000000",
  claimed_total: "0",
  depth_ok_since_ns: (BigInt(state.now_ns) - 18n * HOUR_NS).toString(),
  claimable_now: false,
  claimable_at_ns: (BigInt(state.now_ns) + 6n * HOUR_NS).toString(),
};
const buyQuote = (becomesLarge: boolean) => ({
  tokens_out: "5600000000000000000000000",
  fee_near: "20000000000000000000000",
  creator_fee_near: "6000000000000000000000",
  marked_tokens: "0",
  class_after: becomesLarge ? "large" : "small",
  becomes_large: becomesLarge,
  allowance_after: "7100000000000000000000000",
});

async function openScreen(browser: Browser, width: number, query: string, becomesLarge: boolean, seen: string[]): Promise<Page> {
  const page = await browser.newPage({ viewport: { width, height: 900 } });
  const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "*" };
  page.on("request", (r) => seen.push(r.url()));
  await page.route(/^https?:\/\/(?!127\.0\.0\.1)/, (route) => route.abort());
  await page.route(`${RPC}/**`, async (route) => {
    if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
    const q = JSON.parse(route.request().postData() ?? "{}").params ?? {};
    let answer: object = {};
    if (q.request_type === "call_function") {
      const views: Record<string, unknown> = { ...state.views, fc_account: holderView, fc_creator_fees: creatorFees, storage_balance_bounds: { min: "1250000000000000000000", max: null }, fc_quote_buy: buyQuote(becomesLarge) };
      answer = { result: { result: Array.from(new TextEncoder().encode(JSON.stringify(views[q.method_name as string]))) } };
    } else if (q.request_type === "view_account") answer = { result: state.account };
    else if (q.request_type === "view_access_key_list") answer = { result: { keys: [] } };
    return route.fulfill({ status: 200, contentType: "application/json", headers: cors, body: JSON.stringify({ jsonrpc: "2.0", id: "fc", ...answer }) });
  });
  await page.route(`${FASTNEAR}/**`, async (route) => {
    if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
    const answer = new URL(route.request().url()).pathname === "/v0/account" ? { account_txs: [], resume_token: null } : { transactions: [] };
    return route.fulfill({ status: 200, contentType: "application/json", headers: cors, body: JSON.stringify(answer) });
  });
  await page.goto(`${served.origin}/${query}`);
  return page;
}

function fail(label: string, what: string): never {
  throw new Error(`${label}: ${what}`);
}

/** Controls that are turned on without a handler, and turned-off buttons without a reason. */
async function controlProblems(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const problems: string[] = [];
    for (const n of document.querySelectorAll<HTMLElement>("button, input, select, textarea")) {
      if ((n as HTMLButtonElement).disabled || n.closest("header.screen-header")) continue;
      const form = (n as HTMLButtonElement).form;
      const works = n.hasAttribute("data-handler") || (form?.hasAttribute("data-handler") && (n.tagName !== "BUTTON" || n.getAttribute("type") === "submit"));
      if (!works) problems.push(`enabled ${n.tagName.toLowerCase()}#${n.id} has no handler`);
    }
    for (const b of document.querySelectorAll<HTMLButtonElement>("button[disabled]")) {
      const reason = document.getElementById(b.getAttribute("aria-describedby") ?? "")?.textContent?.trim() ?? "";
      if (reason.length < 10) problems.push(`turned-off button #${b.id} has no reason`);
    }
    return problems;
  });
}

const served = await buildAndServePage();
let browser: Browser | undefined;
let exitCode = 0;
try {
  const executablePath = process.env.FC_BROWSER_PATH || undefined;
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : { channel: "chrome" }) });
  const coinQuery = `?coin=${COIN}&rpc=${encodeURIComponent(RPC)}&fastnear=${encodeURIComponent(FASTNEAR)}&factory=factory.sandbox`;

  for (const width of widths) {
    let label = `coin page at ${width}`;
    const seen: string[] = [];
    let page = await openScreen(browser, width, coinQuery, false, seen);
    await page.waitForSelector("#trade-box");
    await page.waitForSelector("#creator-fees");

    const layout = await page.evaluate(() => {
      const box = (id: string) => {
        const r = document.querySelector(id)?.getBoundingClientRect();
        return r ? { left: r.left, right: r.right, top: r.top, width: r.width } : null;
      };
      const grid = document.querySelector(".coin-page-grid");
      return {
        columns: grid ? getComputedStyle(grid).gridTemplateColumns.split(" ").length : 0,
        sideWidth: box("#trade-box")?.width ?? 0,
        who: box("#who-can-sell"), plan: box("#your-plan"), trade: box("#trade-box"), chart: box("#price-chart"),
        receipts: box("#receipts-section"), holders: box("#holders"), rules: box("#rules"),
        lines: box("#summary-lines"), fees: box("#creator-fees"),
        width: document.documentElement.scrollWidth,
      };
    });
    if (layout.width > width) fail(label, `the document is ${layout.width} px wide`);
    if (width === 1440) {
      if (layout.columns !== 2) fail(label, `${layout.columns} grid columns, want 2`);
      if (Math.round(layout.sideWidth) !== 400) fail(label, `the side column is ${layout.sideWidth} px, want 400`);
      const { who, plan, trade, chart, receipts, holders, rules } = layout;
      if (!who || !plan || !trade || !chart || !receipts || !holders || !rules) fail(label, `a card is missing: ${JSON.stringify(layout)}`);
      if (!(who!.right <= plan!.left + 1 && Math.abs(who!.top - plan!.top) < 2)) fail(label, "who can sell must sit left of your plan");
      if (!(chart!.right <= trade!.left + 1 && trade!.top > plan!.top)) fail(label, "the chart must sit left of the trade box, below your plan");
      if (!(receipts!.top > chart!.top && receipts!.right <= trade!.left + 1)) fail(label, "receipts must sit under the chart, left of the trade box");
      if (!(holders!.top > receipts!.top && holders!.width > trade!.width * 2)) fail(label, "holders must span both columns under receipts");
      if (!(rules!.top > holders!.top)) fail(label, "rules must come last");
    } else {
      if (layout.columns !== 1) fail(label, `${layout.columns} grid columns, want 1`);
      // v5 phone order: lines, trade box, your plan, who can sell, fees, receipts, chart, holders, rules.
      const order = [layout.lines, layout.trade, layout.plan, layout.who, layout.fees, layout.receipts, layout.chart, layout.holders, layout.rules].map((b) => b?.top ?? null);
      if (!order.every((t, i) => t !== null && (i === 0 || t > (order[i - 1] as number)))) fail(label, `the phone order is wrong: ${JSON.stringify(order)}`);
    }

    const problems = await controlProblems(page);
    if (problems.length > 0) fail(label, problems.join("; "));
    const off = await page.evaluate(() => {
      const state = (id: string) => ({ off: (document.getElementById(id) as HTMLButtonElement | null)?.disabled === true, reason: document.getElementById(`${id}-reason`)?.textContent ?? "" });
      return { connect: state("trade-connect"), buy: state("trade-buy-button"), claim: state("claim-fees-button") };
    });
    for (const [name, s] of Object.entries(off)) if (!s.off) fail(label, `${name} must be turned off`);
    if (!off.buy.reason.includes("If this account never held this coin, the buy also pays a storage deposit of 0.00125 NEAR.")) fail(label, `the buy reason must name the storage deposit from the coin: ${off.buy.reason}`);

    // Buy preview: the warning comes from the view.
    await page.fill("#trade-buy-account", "you.testnet");
    await page.fill("#trade-buy-amount", "1");
    await page.click("#trade-buy-preview");
    await page.waitForSelector("#trade-buy-lines dd");
    if ((await page.locator("#large-buy-warning").count()) !== 0) fail(label, "no warning when becomes_large is false");
    await page.close();

    label = `coin page, buy that makes you large, at ${width}`;
    page = await openScreen(browser, width, coinQuery, true, seen);
    await page.waitForSelector("#trade-box");
    await page.fill("#trade-buy-account", "you.testnet");
    await page.fill("#trade-buy-amount", "1");
    await page.click("#trade-buy-preview");
    await page.waitForSelector("#large-buy-warning");
    const warn = (await page.textContent("#large-buy-warning")) ?? "";
    if (!/large holder/.test(warn)) fail(label, `the warning text is wrong: ${warn}`);
    const after = await controlProblems(page);
    if (after.length > 0) fail(label, after.join("; "));
    if (width === 390 && (await page.evaluate(() => document.documentElement.scrollWidth)) > width) fail(label, "the document is too wide");
    if (seen.some((u) => !u.startsWith("http://127.0.0.1") && !/fonts\.(googleapis|gstatic)\.com/.test(u) && !u.startsWith("data:"))) {
      fail(label, `outside requests: ${seen.join(", ")}`);
    }
    await page.close();

    label = `launch form at ${width}`;
    page = await openScreen(browser, width, "?view=launch", false, []);
    await page.waitForSelector("#launch-form");
    const launch = await page.evaluate(() => ({
      max: document.getElementById("launch-description")?.getAttribute("maxlength"),
      width: document.documentElement.scrollWidth,
      launchOff: (document.getElementById("launch-button") as HTMLButtonElement | null)?.disabled === true,
    }));
    if (launch.max !== String(LAUNCH_LIMITS.description)) fail(label, `the description limit is ${launch.max}`);
    if (!launch.launchOff) fail(label, "the launch button must be turned off");
    if (launch.width > width) fail(label, `the document is ${launch.width} px wide`);
    const launchProblems = await controlProblems(page);
    if (launchProblems.length > 0) fail(label, launchProblems.join("; "));
    await page.close();
  }
  console.log("turned-off screens ok: coin page grid, trade box, claim panel and launch form at 1440 and 390");
} catch (err) {
  console.error(`check-turned-off-screens-in-browser: ${err instanceof Error ? err.message : String(err)}`);
  exitCode = 1;
} finally {
  await browser?.close();
  await served.close();
}
process.exit(exitCode);
