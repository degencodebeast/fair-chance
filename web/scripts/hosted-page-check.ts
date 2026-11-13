// The checks for a hosted coin page: it loads, the early banner shows, the receipt list renders,
// the stats are complete and the three coin account checks pass. It reads only. It stops at the
// first 403 or 429 answer and never retries.
import type { Page } from "playwright-core";

const loadWaitMs = 90_000;

export interface HostedPageReport {
  coin: string;
  status: string;
  receiptRows: number;
  checks: { id: string; status: string }[];
}

export async function checkHostedPage(page: Page, url: string): Promise<HostedPageReport> {
  const errors: string[] = [];
  let refused = "";
  page.on("pageerror", (e) => errors.push(`uncaught: ${e.message}`));
  page.on("response", (response) => {
    if ((response.status() === 403 || response.status() === 429) && refused === "") refused = `HTTP ${response.status()} from ${new URL(response.url()).host}`;
  });
  const own = new URL(url).host;
  page.on("console", (msg) => {
    if (msg.type() !== "error") return;
    const at = new URL(msg.location().url || url);
    // A host with no favicon answers 404 for it; that is not a page error.
    if (at.host === own && at.pathname !== "/favicon.ico") errors.push(msg.text());
  });

  await page.goto(url);
  await page
    .waitForFunction(() => /sell receipts|Values cannot load/.test(document.querySelector("#status")?.textContent ?? ""), null, { timeout: loadWaitMs })
    .catch(() => undefined);
  if (refused !== "") throw new Error(`${refused}. Stop and report; do not retry.`);

  const status = (await page.textContent("#status").catch(() => null)) ?? "";
  if (status.startsWith("Values cannot load")) throw new Error(`the page shows an error: ${status}`);
  if (!/sell receipts/.test(status)) throw new Error(`the page did not finish loading (status "${status}")`);
  if ((await page.$("#early-banner")) === null) throw new Error("the early banner is missing");
  if ((await page.$("#receipts")) === null) throw new Error("the receipt list is missing");
  const receiptRows = (await page.$$("#receipts li[data-receipt-id]")).length;

  const stats = await page.$$eval("#stats .stat-value", (nodes) => nodes.map((n) => n.textContent ?? ""));
  if (stats.length !== 4 || stats.some((s) => s === "")) throw new Error(`the stats row is incomplete: ${JSON.stringify(stats)}`);
  const checks = await page.$$eval("#chain-checks li", (nodes) =>
    nodes.map((n) => ({ id: n.getAttribute("data-check") ?? "", status: n.getAttribute("data-status") ?? "" })),
  );
  if (checks.length !== 3 || checks.some((c) => c.status !== "pass")) throw new Error(`the coin account checks did not all pass: ${JSON.stringify(checks)}`);
  if ((await page.$("#chain-warning")) !== null) throw new Error("the page shows the account warning");

  await page.waitForTimeout(1_500);
  if (errors.length > 0) throw new Error(`console errors: ${errors.join(" | ")}`);
  return { coin: (await page.textContent("#coin")) ?? "", status, receiptRows, checks };
}
