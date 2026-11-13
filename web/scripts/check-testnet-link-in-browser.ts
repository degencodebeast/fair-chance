// Opens the built coin page in headless Google Chrome with a `?network=testnet` link and answers
// every outside host from the recorded coin page state, so no request leaves the machine. The page
// must load, and the browser must have sent its reads to the fixed testnet RPC and FastNEAR hosts
// and to nothing else. This catches errors that only the browser raises (for example fetch called
// with the wrong receiver), which the unit tests miss because they give the page their own fetch.
//   node scripts/check-testnet-link-in-browser.ts
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import { buildAndServePage } from "./page-server.ts";

const state = JSON.parse(readFileSync(join(fileURLToPath(new URL("../../testdata/coin-page-states/normal.json", import.meta.url))), "utf8"));
const coin = "moth.factory.testnet";
const rpcHost = new URL(KNOWN_NETWORKS.testnet.rpcUrl).host;
const fastNearHost = new URL(KNOWN_NETWORKS.testnet.fastNearUrl).host;

const served = await buildAndServePage();
let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;
let exitCode = 0;
try {
  const executablePath = process.env.FC_BROWSER_PATH || undefined;
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : { channel: "chrome" }) });
  const page = await browser.newPage();
  const hostsSeen = new Set<string>();
  const strangers: string[] = [];
  await page.route(/^https?:\/\/(?!127\.0\.0\.1)/, async (route) => {
    const url = new URL(route.request().url());
    if (url.host !== rpcHost && url.host !== fastNearHost) {
      // Fonts and other page assets are not reads; only a read to an unknown host is a failure.
      if (!/fonts\.(googleapis|gstatic)\.com$/.test(url.host)) strangers.push(url.host);
      return route.abort();
    }
    hostsSeen.add(url.host);
    const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "*" };
    if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers: cors });
    const body = JSON.parse(route.request().postData() ?? "{}");
    let answer: unknown = {};
    if (url.host === fastNearHost) {
      answer = url.pathname === "/v0/account" ? { account_txs: [], resume_token: null, txs_count: 0 } : { transactions: [] };
    } else {
      const q = body.params ?? {};
      if (q.request_type === "call_function") {
        const view = state.views[q.method_name as string];
        answer = { result: { result: Array.from(new TextEncoder().encode(JSON.stringify(view))) } };
      } else if (q.request_type === "view_account") answer = { result: state.account };
      else if (q.request_type === "view_access_key_list") answer = { result: { keys: [] } };
    }
    const reply = url.host === fastNearHost ? answer : { jsonrpc: "2.0", id: "fc", ...(answer as object) };
    return route.fulfill({ status: 200, contentType: "application/json", headers: cors, body: JSON.stringify(reply) });
  });
  await page.goto(`${served.origin}/?coin=${coin}&network=testnet`);
  await page.waitForFunction(
    () => /sell receipts|Values cannot load/.test(document.querySelector("#status")?.textContent ?? ""),
    null,
    { timeout: 60_000 },
  );
  const status = (await page.textContent("#status")) ?? "";
  if (status.startsWith("Values cannot load")) throw new Error(`the testnet link does not load: ${status}`);
  if (!hostsSeen.has(rpcHost) || !hostsSeen.has(fastNearHost)) {
    throw new Error(`the page did not read from both fixed hosts: ${[...hostsSeen].join(", ")}`);
  }
  if (strangers.length > 0) throw new Error(`the page called other hosts: ${strangers.join(", ")}`);
  console.log(JSON.stringify({ status, hosts: [...hostsSeen] }));
} catch (err) {
  console.error(`check-testnet-link-in-browser: ${err instanceof Error ? err.message : String(err)}`);
  exitCode = 1;
} finally {
  await browser?.close();
  await served.close();
}
process.exit(exitCode);
