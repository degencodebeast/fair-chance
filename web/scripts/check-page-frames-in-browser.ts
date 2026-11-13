// Opens the built coin page in headless Google Chrome and checks the frame of the loading state
// and of the error state at widths 1440 and 390. Every outside host is stubbed: the loading state
// holds the RPC request open, the error state answers it with HTTP 503. No request leaves the
// machine. Each card must say "Loading" or "Not loaded", the error title must name the network,
// and at 390 the document must be no wider than 390.
//   node scripts/check-page-frames-in-browser.ts
import { chromium } from "playwright-core";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import { buildAndServePage } from "./page-server.ts";

const coin = "moth.factory.testnet";
const rpcHost = new URL(KNOWN_NETWORKS.testnet.rpcUrl).host;
const widths = [1440, 390];

const served = await buildAndServePage();
let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;
let exitCode = 0;
try {
  const executablePath = process.env.FC_BROWSER_PATH || undefined;
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : { channel: "chrome" }) });
  for (const state of ["Loading", "Not loaded"] as const) {
    for (const width of widths) {
      const page = await browser.newPage({ viewport: { width, height: 900 } });
      await page.route(/^https?:\/\/(?!127\.0\.0\.1)/, async (route) => {
        const host = new URL(route.request().url()).host;
        const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "*" };
        if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
        if (host !== rpcHost) return route.abort();
        if (state === "Loading") return; // held open: the page stays in the loading state
        return route.fulfill({ status: 503, headers: cors, body: "unavailable" });
      });
      await page.goto(`${served.origin}/?coin=${coin}&network=testnet`);
      if (state === "Not loaded") await page.waitForSelector("#load-error", { timeout: 60_000 });
      else await page.waitForSelector("#receipts-empty", { timeout: 60_000 });
      const seen = await page.evaluate(() => ({
        stats: [...document.querySelectorAll("#stats .stat-value")].map((n) => n.textContent),
        summary: [...document.querySelectorAll("#summary-lines .num")].map((n) => n.textContent),
        cards: [...document.querySelectorAll(".who-card .who-line")].map((n) => n.textContent),
        receipts: document.querySelector("#receipts-empty")?.textContent ?? null,
        error: document.querySelector("#load-error")?.textContent ?? null,
        width: document.documentElement.scrollWidth,
      }));
      const label = `${state} at ${width}`;
      const all = [...seen.stats, ...seen.summary, ...seen.cards, seen.receipts];
      if (seen.stats.length !== 4 || seen.summary.length !== 3 || seen.cards.length !== 3) throw new Error(`${label}: the frame is missing a card (${JSON.stringify(seen)})`);
      if (!all.every((t) => t === state)) throw new Error(`${label}: a card does not say "${state}" (${JSON.stringify(seen)})`);
      if (state === "Not loaded" && !/Values cannot load from NEAR testnet\./.test(seen.error ?? "")) {
        throw new Error(`${label}: the error does not name the network (${seen.error})`);
      }
      if (seen.width > width) {
        throw new Error(`${label}: the document is ${seen.width} px wide`);
      }
      await page.close();
    }
  }
  console.log("page frames ok: Loading and Not loaded at 1440 and 390");
} catch (err) {
  console.error(`check-page-frames-in-browser: ${err instanceof Error ? err.message : String(err)}`);
  exitCode = 1;
} finally {
  await browser?.close();
  await served.close();
}
process.exit(exitCode);
