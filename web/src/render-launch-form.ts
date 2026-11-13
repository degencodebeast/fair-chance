// Draws the launch form screen. The launch button and the picture button are turned off, with the
// reason beside each. Typing updates the counts, the error list and the preview. Nothing here
// signs or sends. Text comes from the launch form model.
import { el, type Child } from "./dom-elements.ts";
import { listen } from "./dom-listen.ts";
import { LAUNCH_LIMITS, launchFormModel } from "./launch-form-model.ts";
import { screenHeader, turnedOffButton } from "./screen-chrome.ts";

const PICTURE_REASON = "Choosing a picture is turned off in this version. A picture must be a WebP image under 8 KB.";

function field(label: string, id: string, max: number, attrs: Record<string, string> = {}): { box: HTMLElement; input: HTMLInputElement; count: HTMLElement } {
  const input = el("input", { id, type: "text", maxlength: String(max), autocomplete: "off", ...attrs });
  const count = el("span", { id: `${id}-count`, class: "num muted" });
  return { box: el("label", { class: "launch-field", for: id }, el("b", {}, label), input, count), input, count };
}

export function drawLaunchForm(app: HTMLElement, href: string): void {
  const name = field("Name", "launch-name", LAUNCH_LIMITS.name);
  const ticker = field("Ticker", "launch-ticker", LAUNCH_LIMITS.ticker, { spellcheck: "false" });
  const description = field("One line about the coin", "launch-description", LAUNCH_LIMITS.description);
  const buy = el("input", { id: "launch-buy", type: "text", inputmode: "decimal", autocomplete: "off", placeholder: "4.40", "aria-describedby": "launch-buy-help" });
  const buyHelp = el("p", { id: "launch-buy-help", class: "muted" });
  const errors = el("ul", { id: "launch-errors", class: "launch-errors", role: "status" });
  const feeNote = el("p", { class: "muted" });
  const buttonSlot = el("div", { id: "launch-action" });
  const previewBox = el("section", { id: "launch-preview", "aria-label": "Launch preview", class: "card" });

  const refresh = (): void => {
    const m = launchFormModel({ name: name.input.value, ticker: ticker.input.value, description: description.input.value, openingBuy: buy.value });
    name.count.textContent = m.counts.name;
    ticker.count.textContent = m.counts.ticker;
    description.count.textContent = m.counts.description;
    buyHelp.textContent = m.buyHelp;
    feeNote.textContent = m.feeNote;
    errors.replaceChildren(...m.errors.map((e) => el("li", { class: "tone-limit" }, e)));
    buttonSlot.replaceChildren(turnedOffButton("launch-button", m.button.label, m.button.reason));
    previewBox.replaceChildren(
      el("h2", {}, m.preview.name === "" ? "Your coin" : m.preview.name),
      el("p", { class: "num" }, m.preview.ticker === "" ? "TICKER" : m.preview.ticker),
      el("p", {}, m.preview.description === "" ? "No description." : m.preview.description),
      el("p", { class: "muted" }, m.preview.openingBuy === null ? "No opening buy." : `Opening buy ${m.preview.openingBuy}`),
    );
  };
  for (const input of [name.input, ticker.input, description.input, buy]) listen(input, "input", refresh);

  const form = el(
    "form",
    { id: "launch-form", class: "card launch-form" },
    el("div", { class: "launch-picture" }, el("span", { class: "muted num" }, "coin picture"), turnedOffButton("launch-picture-button", "Choose", PICTURE_REASON)),
    name.box,
    ticker.box,
    description.box,
    el("div", { class: "launch-field" }, el("label", { for: "launch-buy" }, el("b", {}, "Your opening buy "), el("span", { class: "muted" }, "(optional)")), el("div", { class: "launch-buy-row" }, buy, el("span", { class: "num" }, "NEAR")), buyHelp),
    errors,
    buttonSlot,
    feeNote,
  );
  listen(form, "submit", (event) => event.preventDefault());

  const children: Child[] = [
    screenHeader(href, "launch"),
    el(
      "div",
      {},
      el("h1", { id: "launch-title" }, "Launch a coin"),
      el("p", {}, "Picture, name, ticker and one line. Every coin gets the same rules, and nobody can change them after launch."),
    ),
    el("div", { class: "launch-grid" }, form, previewBox),
  ];
  app.replaceChildren(...children.filter((c): c is Exclude<Child, null | false> => Boolean(c)));
  refresh();
}
