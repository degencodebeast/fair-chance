// Draws the "Recorded testnet run" section. Text goes in as text nodes only.
import { el } from "./dom-elements.ts";
import type { RecordedRun } from "./recorded-run.ts";

export function drawRecordedRun(run: RecordedRun): HTMLElement {
  const head = el("div", { class: "section-head" }, el("h2", {}, "Recorded testnet run"), el("span", { class: "muted" }, "Recorded on testnet. Not read from the chain now."));
  if (!run.ok) {
    return el("section", { id: "recorded-run", class: "card", "aria-label": "Recorded testnet run" }, head, el("p", { class: "note", id: "recorded-run-error" }, `The recorded run cannot load: ${run.reason}.`));
  }
  return el(
    "section",
    { id: "recorded-run", class: "card", "aria-label": "Recorded testnet run" },
    head,
    el(
      "ol",
      { id: "recorded-receipts" },
      ...run.rows.map((r) =>
        el(
          "li",
          { class: "receipt", "data-tx-hash": r.txHash },
          el("div", { class: "receipt-body" }, el("p", { class: "receipt-sentence" }, r.text), el("a", { href: r.link, target: "_blank", rel: "noopener noreferrer" }, "See it on the explorer")),
        ),
      ),
    ),
  );
}
