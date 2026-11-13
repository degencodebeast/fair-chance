// Prints, as JSON, the receipts the page shows for a page URL:
//   node scripts/read-receipts.ts "http://localhost/?coin=<coin>&rpc=<url-encoded RPC URL>"
import { loadReceiptsPage } from "../src/page.ts";

const href = process.argv[2];
if (!href) {
  console.error("usage: node scripts/read-receipts.ts <page URL with ?coin=..&rpc=..>");
  process.exit(2);
}
try {
  console.log(JSON.stringify(await loadReceiptsPage(href)));
} catch (err) {
  console.error(`read-receipts: ${err instanceof Error ? err.message : String(err)}`);
  process.exit(1);
}
