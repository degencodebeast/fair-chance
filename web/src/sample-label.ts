import { el } from "./dom-elements.ts";

/** The label a card carries when its values are not read from the chain. */
export function sampleLabel(text = "Sample data. Made-up values, not read from the chain."): HTMLElement {
  return el("span", { class: "sample-label" }, text);
}
