// Draws the launch form screen. The launch button and the picture button are turned off, with the
// reason beside each. Typing updates the counts, the error list and the preview. Nothing here
// signs or sends. Text comes from the launch form model.
import { el, type Child } from "./dom-elements.ts";
import { listen } from "./dom-listen.ts";
import { iconToWebp, type IconResult } from "./icon-to-webp.ts";
import { LAUNCH_LIMITS, launchFormModel, type LaunchArgs } from "./launch-form-model.ts";
import { launchNameCheckFromLink, type LaunchNameCheck } from "./account-name-check.ts";
import { screenHeader, turnedOffButton } from "./screen-chrome.ts";

/** The picture seams: the converter (a test passes its own) and the receiver of the launch arguments, which a wiring part uses to sign. */
export interface LaunchFormPicture {
  convert?: (file: File) => Promise<IconResult>;
  onArgs?: (args: LaunchArgs | null) => void;
}

function field(label: string, id: string, max: number, attrs: Record<string, string> = {}): { box: HTMLElement; input: HTMLInputElement; count: HTMLElement } {
  const input = el("input", { id, type: "text", maxlength: String(max), autocomplete: "off", ...attrs });
  const count = el("span", { id: `${id}-count`, class: "num muted" });
  return { box: el("label", { class: "launch-field", for: id }, el("b", {}, label), input, count), input, count };
}

export function drawLaunchForm(app: HTMLElement, href: string, nameCheck: LaunchNameCheck = launchNameCheckFromLink(href), picture: LaunchFormPicture = {}): void {
  const convert = picture.convert ?? iconToWebp;
  let icon: string | null = null;
  let latestPicture = 0;
  let pictureState: NonNullable<Parameters<typeof launchFormModel>[0]["picture"]> = { state: "none" };
  const pictureInput = el("input", { id: "launch-picture-input", type: "file", accept: "image/png,image/jpeg,image/gif,image/webp,image/svg+xml", "aria-describedby": "launch-picture-status" });
  const pictureStatus = el("span", { id: "launch-picture-status", class: "muted", role: "status" }, "Any picture. It is shrunk to a WebP image under 8 KB.");
  const name = field("Name", "launch-name", LAUNCH_LIMITS.name);
  const ticker = field("Ticker", "launch-ticker", LAUNCH_LIMITS.ticker, { spellcheck: "false" });
  const description = field("One line about the coin", "launch-description", LAUNCH_LIMITS.description);
  const buy = el("input", { id: "launch-buy", type: "text", inputmode: "decimal", autocomplete: "off", placeholder: "4.40", "aria-describedby": "launch-buy-help" });
  const buyHelp = el("p", { id: "launch-buy-help", class: "muted" });
  const nameStatus = el("p", { id: "launch-name-check", class: "muted", role: "status" });
  const errors = el("ul", { id: "launch-errors", class: "launch-errors", role: "status" });
  const feeNote = el("p", { class: "muted" });
  const buttonSlot = el("div", { id: "launch-action" });
  const previewBox = el("section", { id: "launch-preview", "aria-label": "Launch preview", class: "card" });

  const refresh = (): void => {
    const m = launchFormModel({ name: name.input.value, ticker: ticker.input.value, description: description.input.value, openingBuy: buy.value, icon, picture: pictureState });
    picture.onArgs?.(m.launchArgs);
    name.count.textContent = m.counts.name;
    ticker.count.textContent = m.counts.ticker;
    description.count.textContent = m.counts.description;
    buyHelp.textContent = m.buyHelp;
    feeNote.textContent = m.feeNote;
    errors.replaceChildren(...m.errors.map((e) => el("li", { class: "tone-limit" }, e)));
    buttonSlot.replaceChildren(turnedOffButton("launch-button", m.button.label, m.button.reason));
    previewBox.replaceChildren(
      ...(icon === null ? [] : [el("img", { id: "launch-picture-preview", class: "launch-picture-preview", src: icon, alt: "Coin picture preview", width: "64", height: "64" })]),
      el("h2", {}, m.preview.name === "" ? "Your coin" : m.preview.name),
      el("p", { class: "num" }, m.preview.ticker === "" ? "TICKER" : m.preview.ticker),
      el("p", {}, m.preview.description === "" ? "No description." : m.preview.description),
      el("p", { class: "muted" }, m.preview.openingBuy === null ? "No opening buy." : `Opening buy ${m.preview.openingBuy}`),
    );
  };
  let latestCheck = 0;
  const checkName = (): void => {
    const mine = ++latestCheck;
    const label = launchFormModel({ name: name.input.value, ticker: ticker.input.value, description: description.input.value, openingBuy: buy.value }).accountLabel;
    if (label === null) {
      nameStatus.textContent = "";
      return;
    }
    if (nameCheck.factory === null || nameCheck.checkName === null) {
      nameStatus.textContent = "Not checked: this link names no factory.";
      return;
    }
    const accountId = `${label}.${nameCheck.factory}`;
    nameStatus.textContent = `Checking ${accountId}.`;
    nameCheck.checkName(accountId).then(
      (state) => {
        if (mine === latestCheck) nameStatus.textContent = state === "free" ? `${accountId} is free.` : `${accountId} is taken. Pick another ticker.`;
      },
      (e: unknown) => {
        if (mine === latestCheck) nameStatus.textContent = `Not checked: the name could not be read. ${e instanceof Error ? e.message : String(e)}`;
      },
    );
  };
  listen(pictureInput, "change", () => {
    const file = pictureInput.files?.[0];
    const mine = ++latestPicture;
    icon = null;
    pictureState = file === undefined ? { state: "none" } : { state: "pending" };
    refresh();
    if (file === undefined) {
      pictureStatus.textContent = "No picture. The coin gets none.";
      return;
    }
    pictureStatus.textContent = "Shrinking the picture.";
    convert(file).then(
      (result) => {
        if (mine !== latestPicture) return;
        icon = result.ok ? result.dataUrl : null;
        pictureState = result.ok ? { state: "ready" } : { state: "refused", reason: result.reason };
        pictureStatus.textContent = result.ok ? `Picture ready: ${result.dataUrl.length} of 8191 characters.` : result.reason;
        refresh();
      },
      () => {
        if (mine !== latestPicture) return;
        const reason = "This page could not read this file as a picture. Choose a PNG, JPEG, GIF, WebP or SVG file.";
        pictureState = { state: "refused", reason };
        pictureStatus.textContent = reason;
        refresh();
      },
    );
  });
  listen(ticker.input, "change", checkName);
  listen(ticker.input, "input", () => {
    latestCheck++;
    nameStatus.textContent = "";
  });
  for (const input of [name.input, ticker.input, description.input, buy]) listen(input, "input", refresh);

  const form = el(
    "form",
    { id: "launch-form", class: "card launch-form" },
    el("div", { class: "launch-picture" }, el("label", { class: "muted num", for: "launch-picture-input" }, "coin picture"), pictureInput, pictureStatus),
    name.box,
    ticker.box,
    nameStatus,
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
