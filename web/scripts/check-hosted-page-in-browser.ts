// Opens a hosted coin page in headless Chrome and runs the read-only page checks. No keys, no
// wallet, no stub: the page reads the real chain. The URL comes from the command line or from
// FC_HOSTED_URL, never from a file. Only the host is printed.
//   node scripts/check-hosted-page-in-browser.ts [url]
import { chromium } from "playwright-core";
import { checkHostedPage, withoutLinkValues } from "./hosted-page-check.ts";

const raw = process.argv[2] ?? process.env.FC_HOSTED_URL ?? "";
let url: URL;
try {
  url = new URL(raw);
} catch {
  console.error("check-hosted-page-in-browser: give the page URL as the argument or in FC_HOSTED_URL");
  process.exit(2);
}
const local = url.hostname === "127.0.0.1" || url.hostname === "localhost";
if (url.protocol !== "https:" && !(url.protocol === "http:" && local)) {
  console.error("check-hosted-page-in-browser: the URL must be https (http only on 127.0.0.1)");
  process.exit(2);
}
let exitCode = 0;
const executablePath = process.env.FC_BROWSER_PATH || undefined;
const browser = await chromium.launch({ headless: true, ...(executablePath ? { executablePath } : { channel: "chrome" }) });
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const report = await checkHostedPage(page, url.href);
  console.log(withoutLinkValues(`hosted page ok on ${url.host}: ${JSON.stringify(report)}`, url.href));
} catch (err) {
  console.error(withoutLinkValues(`check-hosted-page-in-browser (${url.host}): ${err instanceof Error ? err.message : String(err)}`, url.href));
  exitCode = 1;
} finally {
  await browser.close();
}
process.exit(exitCode);
