// Builds the coin page, serves it on a free local port, opens it in headless Google Chrome and
// waits until the page shows one sell receipt. Prints that receipt row and the page facts
// (stats, code hash, coin account checks) as JSON.
//   node scripts/read-receipt-in-browser.ts "?coin=<coin>&rpc=<url-encoded RPC URL>" <receipt id | any> [screenshot.png]
// "any" waits for the first receipt row. Each run builds into its own temporary folder and the
// browser uses a fresh profile, so runs at the same time share nothing. Set FC_BROWSER_PATH to use
// a Chrome binary that is not at the standard install place. Nothing is downloaded.
import { createServer, type Server } from "node:http";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";
import { chromium } from "playwright-core";

const webRoot = fileURLToPath(new URL("..", import.meta.url));
const waitMs = 90_000;

const [query, wantReceipt, screenshotPath] = process.argv.slice(2);
if (!query?.startsWith("?") || !wantReceipt) {
  console.error('usage: node scripts/read-receipt-in-browser.ts "?coin=..&rpc=.." <receipt id | any> [screenshot.png]');
  process.exit(2);
}
if (!/^[A-Za-z0-9]+$/.test(wantReceipt)) {
  console.error(`read-receipt-in-browser: receipt id must be base58 or "any", got ${wantReceipt}`);
  process.exit(2);
}

const contentTypes: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
};

function serveFolder(folder: string): Promise<{ server: Server; origin: string }> {
  const server = createServer(async (req, res) => {
    const path = new URL(req.url ?? "/", "http://localhost").pathname;
    const relative = normalize(path === "/" ? "/index.html" : path);
    if (relative.includes("..")) {
      res.writeHead(400).end();
      return;
    }
    try {
      const body = await readFile(join(folder, relative));
      res.writeHead(200, { "content-type": contentTypes[extname(relative)] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (address === null || typeof address === "string") return reject(new Error("no port"));
      resolve({ server, origin: `http://127.0.0.1:${address.port}` });
    });
  });
}

const scratch = await mkdtemp(join(tmpdir(), "fair-chance-page-"));
let server: Server | undefined;
let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;
let exitCode = 0;
try {
  const outDir = join(scratch, "page");
  await build({
    root: webRoot,
    configFile: false,
    logLevel: "silent",
    cacheDir: join(scratch, "vite-cache"),
    build: { outDir, emptyOutDir: true },
  });
  const served = await serveFolder(outDir);
  server = served.server;

  const executablePath = process.env.FC_BROWSER_PATH || undefined;
  browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : { channel: "chrome" }) });
  const page = await browser.newPage();
  const consoleErrors: string[] = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") consoleErrors.push(msg.text());
  });

  const pageUrl = `${served.origin}/${query}`;
  await page.goto(pageUrl);
  const rowSelector =
    wantReceipt === "any" ? "#receipts li[data-receipt-id]" : `#receipts li[data-receipt-id="${wantReceipt}"]`;
  // The page either shows the row or puts "Values cannot load: ..." in #status.
  await page.waitForFunction(
    (selector) =>
      document.querySelector(selector) !== null ||
      (document.querySelector("#status")?.textContent ?? "").startsWith("Values cannot load"),
    rowSelector,
    { timeout: waitMs },
  );
  const status = (await page.textContent("#status")) ?? "";
  const loadFailed = status.startsWith("Values cannot load");
  if (screenshotPath) await page.screenshot({ path: screenshotPath, fullPage: true });
  const row = await page.$(rowSelector);
  if (row === null) throw new Error(`the page shows no receipt row: ${status} ${consoleErrors.join(" | ")}`);
  if (loadFailed) throw new Error(`the page shows an error: ${status}`);

  // The coin page facts the run must see: four stats with values, the rules with the code hash,
  // and the three coin account checks. A link that names the known factory and hash must pass.
  const stats = await page.$$eval("#stats .stat", (nodes) =>
    nodes.map((n) => ({
      label: n.querySelector(".stat-label")?.textContent ?? "",
      value: n.querySelector(".stat-value")?.textContent ?? "",
    })),
  );
  const checks = await page.$$eval("#chain-checks li", (nodes) =>
    nodes.map((n) => ({ id: n.getAttribute("data-check") ?? "", status: n.getAttribute("data-status") ?? "" })),
  );
  const codeHash = (await page.textContent("#code-hash")) ?? "";
  const warning = await page.$("#chain-warning");
  if (stats.length !== 4 || stats.some((s) => s.value === "")) throw new Error(`the stats row is incomplete: ${JSON.stringify(stats)}`);
  if (codeHash === "") throw new Error("the rules show no code hash");
  if (checks.length !== 3) throw new Error(`expected 3 coin account checks, got ${JSON.stringify(checks)}`);
  if (query.includes("factory=") && query.includes("code_hash=") && (warning !== null || checks.some((c) => c.status !== "pass"))) {
    throw new Error(`the coin account checks did not pass: ${JSON.stringify(checks)}`);
  }
  // Phone width: no sideways scroll at 390 px, and the cards follow the phone order.
  await page.setViewportSize({ width: 390, height: 844 });
  const phone = await page.evaluate(() => {
    const top = (id: string) => document.querySelector(id)?.getBoundingClientRect().top ?? null;
    return {
      width: document.documentElement.scrollWidth,
      order: ["#summary-lines", "#trade-box", "#your-plan", "#who-can-sell", "#creator-fees", "#receipts-section", "#price-chart", "#holders", "#rules"].map(top),
    };
  });
  if (phone.width > 390) throw new Error(`the page is ${phone.width} px wide at 390 px`);
  const ordered = phone.order.every((t, i) => t !== null && (i === 0 || t > (phone.order[i - 1] as number)));
  if (!ordered) throw new Error(`the phone card order is wrong: ${JSON.stringify(phone.order)}`);
  // The text of a row is "Block <n>: <sentence>", the same as the command-line read.
  const text = await row.evaluate((li) => `Block ${li.getAttribute("data-block")}: ${li.querySelector(".receipt-sentence")?.textContent ?? ""}`);
  console.log(
    JSON.stringify({
      coin: await page.textContent("#coin"),
      coin_account: await page.textContent("#coin-account"),
      receipt_id: await row.getAttribute("data-receipt-id"),
      text,
      status,
      stats,
      code_hash: codeHash,
      checks,
      phone_width: phone.width,
      browser: browser.version(),
    }),
  );
} catch (err) {
  console.error(`read-receipt-in-browser: ${err instanceof Error ? err.message : String(err)}`);
  exitCode = 1;
} finally {
  await browser?.close();
  server?.close();
  await rm(scratch, { recursive: true, force: true });
}
process.exit(exitCode);
