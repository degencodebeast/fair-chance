// Draws the holders table of the coin page. The section starts as "Reading holders" and is filled
// when the account reads finish. If they fail, only this section shows the error; the rest of the
// page keeps its numbers.
import type { ChainReader, CoinEvent } from "./chain.ts";
import { el } from "./dom-elements.ts";
import { loadHolders, type HolderRow } from "./holders-table.ts";

const HEADINGS = ["Account", "Balance", "Class", "Can sell now", "Can send"];

export function holdersPlaceholder(): HTMLElement {
  return el("section", { id: "holders", class: "card", "aria-label": "Holders" }, el("div", { class: "section-head" }, el("h2", {}, "Holders")), el("p", { class: "note", id: "holders-status" }, "Reading holders."));
}

function rowNode(r: HolderRow): HTMLElement {
  return el(
    "li",
    { class: "holder-row" },
    el("span", { class: "num holder-account" }, r.account),
    el("span", { class: "num holder-balance" }, `${r.balance} (${r.share})`),
    el("span", { class: "holder-tags" }, ...r.tags.map((t) => el("span", { class: t === "Large" ? "chip chip-tag-dark" : "chip" }, t))),
    el("span", { class: `num holder-now${r.limited ? " tone-limit" : ""}` }, r.now),
    el("span", { class: "holder-send" }, r.send),
  );
}

export async function fillHolders(section: HTMLElement, chain: ChainReader, events: CoinEvent[], supply: bigint | null, nowNs: bigint): Promise<void> {
  const head = section.querySelector(".section-head")!;
  try {
    if (supply === null) throw new Error("this coin does not give its total supply, so shares cannot be worked out");
    const table = await loadHolders(chain, events, supply, { nowNs });
    const children: (Node | null)[] = [
      head,
      table.rows.length === 0
        ? el("p", { class: "note", id: "holders-empty" }, "No account holds tokens that the coin events name yet.")
        : el("ul", { class: "holder-list" }, el("li", { class: "holder-heading", "aria-hidden": "true" }, ...HEADINGS.map((h) => el("span", {}, h))), ...table.rows.map(rowNode)),
      table.notRead > 0 ? el("p", { class: "muted" }, `Showing the ${table.rows.length === 0 ? "" : "most recently active "}accounts. ${table.notRead} more named in the events were not read.`) : null,
      el("p", { class: "muted" }, "Read from the chain. Accounts are the ones named in the coin events."),
    ];
    section.replaceChildren(...(children.filter(Boolean) as Node[]));
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    section.replaceChildren(head, el("p", { class: "note", id: "holders-error", role: "alert" }, `Holders cannot load. ${message}`));
  }
}
