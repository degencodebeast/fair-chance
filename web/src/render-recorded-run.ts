// Draws the "Recorded testnet run" section. Text goes in as text nodes only.
import { el } from "./dom-elements.ts";
import type { RecordedRun } from "./recorded-run.ts";

export function drawRecordedRun(run: RecordedRun): HTMLElement {
  const head = el("div", { class: "section-head" }, el("h2", {}, "Recorded testnet run"), el("span", { class: "muted" }, "Recorded on testnet. Not read from the chain now."));
  const section = (...body: HTMLElement[]): HTMLElement =>
    el("section", { id: "recorded-run", class: "recorded-run", "aria-label": "Recorded testnet run" }, el("div", { class: "card" }, head, ...body));
  if (!run.ok) {
    return section(el("p", { class: "note", id: "recorded-run-error" }, `The recorded run cannot load: ${run.reason}.`));
  }
  return section(
    el(
      "ol",
      { id: "recorded-receipts" },
      ...run.rows.map((r) =>
        el(
          "li",
          { class: "recorded-row", "data-tx-hash": r.txHash },
          el("p", { class: "receipt-sentence" }, r.text),
          el("a", { href: r.link, target: "_blank", rel: "noopener noreferrer" }, "See it on the explorer"),
        ),
      ),
    ),
  );
}
