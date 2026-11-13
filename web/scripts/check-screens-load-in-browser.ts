// Load check for every screen: landing, coin list, alerts, launch form and the coin page (with its
// trade box and claim panel). For each, at widths 1440 and 390, the page must load with no console
// error and no uncaught error, and the main element must be shown with a heading inside it. Every
// outside host is blocked and the chain is a stub, so no request leaves the machine.
//   node scripts/check-screens-load-in-browser.ts
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium, type Browser } from "playwright-core";
import { buildAndServePage } from "./page-server.ts";

const testdata = (path: string) => JSON.parse(readFileSync(join(fileURLToPath(new URL("../../testdata/", import.meta.url)), path), "utf8"));
const state = testdata("coin-page-states/normal.json");
const holderView = testdata("account-plan-states/cap-full-floor.json").views.fc_account;

const RPC = "http://127.0.0.1:3030";
const FASTNEAR = "http://127.0.0.1:3031";
const COIN = "moth.factory.sandbox";
const widths = [1440, 390];

const screens: { name: string; query: string; ready: string[] }[] = [
  { name: "landing", query: "", ready: ["#landing-headline"] },
  { name: "coin list", query: "?view=coins", ready: [".coin-card"] },
  { name: "alerts", query: "?view=alerts", ready: [".chat-bubble"] },
  { name: "launch form", query: "?view=launch", ready: ["#launch-form"] },
  {
    name: "coin page",
    query: `?coin=${COIN}&rpc=${encodeURIComponent(RPC)}&fastnear=${encodeURIComponent(FASTNEAR)}&factory=factory.sandbox`,
    ready: ["#trade-box", "#creator-fees", "#holders"],
  },
];

const served = await buildAndServePage();
let browser: Browser | undefined;
let exitCode = 0;
try {
  const executablePath = process.env.FC_BROWSER_PATH || undefined;
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : { channel: "chrome" }) });
  const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "*" };
  for (const screen of screens) {
    for (const width of widths) {
      const label = `${screen.name} at ${width}`;
      const page = await browser.newPage({ viewport: { width, height: 900 } });
      const errors: string[] = [];
      page.on("pageerror", (e) => errors.push(`uncaught: ${e.message}`));
      page.on("console", (msg) => {
        // A blocked outside request (fonts) is not a page error; only the page's own host counts.
        if (msg.type() === "error" && new URL(msg.location().url || served.origin).host.startsWith("127.0.0.1")) errors.push(msg.text());
      });
      await page.route(/^https?:\/\/(?!127\.0\.0\.1)/, (route) => route.abort());
      await page.route(`${RPC}/**`, async (route) => {
        if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
        const q = JSON.parse(route.request().postData() ?? "{}").params ?? {};
        let answer: object = {};
        if (q.request_type === "call_function") {
          const view = q.method_name === "fc_account" ? holderView : state.views[q.method_name as string];
          answer = { result: { result: Array.from(new TextEncoder().encode(JSON.stringify(view))) } };
        } else if (q.request_type === "view_account") answer = { result: state.account };
        else if (q.request_type === "view_access_key_list") answer = { result: { keys: [] } };
        return route.fulfill({ status: 200, contentType: "application/json", headers: cors, body: JSON.stringify({ jsonrpc: "2.0", id: "fc", ...answer }) });
      });
      await page.route(`${FASTNEAR}/**`, async (route) => {
        if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
        const answer = new URL(route.request().url()).pathname === "/v0/account" ? { account_txs: [], resume_token: null } : { transactions: [] };
        return route.fulfill({ status: 200, contentType: "application/json", headers: cors, body: JSON.stringify(answer) });
      });
      await page.goto(`${served.origin}/${screen.query}`);
      for (const selector of screen.ready) {
        try {
          await page.waitForSelector(selector, { timeout: 30_000 });
        } catch {
          throw new Error(`${label}: ${selector} never showed`);
        }
      }
      const main = await page.evaluate(() => {
        const m = document.querySelector("main");
        const h = m?.querySelector("h1, h2");
        const box = m?.getBoundingClientRect();
        const visible = (n: Element | null | undefined) => {
          if (!n) return false;
          const r = n.getBoundingClientRect();
          const s = getComputedStyle(n);
          return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
        };
        return { present: m !== null, shown: visible(m), heading: visible(h), height: box?.height ?? 0 };
      });
      if (!main.present) throw new Error(`${label}: there is no main element`);
      if (!main.shown || main.height < 100) throw new Error(`${label}: the main element is not shown (height ${main.height})`);
      if (!main.heading) throw new Error(`${label}: the main element shows no heading`);
      if (errors.length > 0) throw new Error(`${label}: console errors: ${errors.join(" | ")}`);
      await page.close();
    }
  }
  console.log("screens load ok: landing, coin list, alerts, launch form and coin page at 1440 and 390, no console error, main shown");
} catch (err) {
  console.error(`check-screens-load-in-browser: ${err instanceof Error ? err.message : String(err)}`);
  exitCode = 1;
} finally {
  await browser?.close();
  await served.close();
}
process.exit(exitCode);
