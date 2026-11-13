import { loadReceiptsPage } from "./page.ts";

const status = document.querySelector<HTMLParagraphElement>("#status")!;
const list = document.querySelector<HTMLOListElement>("#receipts")!;
const title = document.querySelector<HTMLHeadingElement>("#coin")!;

loadReceiptsPage(location.href)
  .then((page) => {
    title.textContent = page.coin;
    status.textContent = `${page.receipts.length} sell receipts from block ${page.launchBlock}, read from the chain.`;
    for (const row of page.receipts) {
      const item = document.createElement("li");
      item.dataset.receiptId = row.receipt_id;
      item.textContent = `Block ${row.block}: ${row.text}`;
      list.append(item);
    }
  })
  .catch((err: unknown) => {
    status.textContent = `Values cannot load: ${err instanceof Error ? err.message : String(err)}`;
  });
