// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { LAUNCH_LIMITS } from "../src/launch-form-model.ts";
import { drawLaunchForm } from "../src/render-launch-form.ts";
import { expectNoEnabledControlWithoutHandler, turnedOffReasons } from "./dom-controls.ts";

function form() {
  const app = document.createElement("main");
  document.body.replaceChildren(app);
  drawLaunchForm(app, "http://localhost/?view=launch");
  return app;
}
const type = (app: HTMLElement, id: string, value: string) => {
  const input = app.querySelector<HTMLInputElement>(id)!;
  input.value = value;
  input.dispatchEvent(new Event("input"));
};

describe("launch form", () => {
  it("gives each field its length limit, the description the same as the model", () => {
    const app = form();
    expect(app.querySelector("#launch-description")!.getAttribute("maxlength")).toBe(String(LAUNCH_LIMITS.description));
    expect(app.querySelector("#launch-name")!.getAttribute("maxlength")).toBe(String(LAUNCH_LIMITS.name));
    expect(app.querySelector("#launch-ticker")!.getAttribute("maxlength")).toBe(String(LAUNCH_LIMITS.ticker));
  });

  it("has no enabled control without a handler", () => {
    expectNoEnabledControlWithoutHandler(form());
  });

  it("turns off launch and the picture button, each with a reason", () => {
    const reasons = turnedOffReasons(form());
    expect(Object.keys(reasons).sort()).toEqual(["launch-button", "launch-picture-button"]);
    expect(reasons["launch-button"]).toMatch(/turned off/);
  });

  it("updates the count, the preview and the button label as the visitor types", () => {
    const app = form();
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "moth");
    type(app, "#launch-description", "A lamp for moths.");
    expect(app.querySelector("#launch-name-count")?.textContent).toBe("9/32");
    expect(app.querySelector("#launch-preview")?.textContent).toContain("Moth Lamp");
    expect(app.querySelector("#launch-preview")?.textContent).toContain("MOTH");
    expect(app.querySelector("#launch-button")?.textContent).toBe("Launch Moth Lamp");
  });

  it("shows an error for a field that breaks a limit", () => {
    const app = form();
    type(app, "#launch-name", "Moth Lamp");
    type(app, "#launch-ticker", "MO TH");
    expect(app.querySelector("#launch-errors")?.textContent).toContain("letters, digits, - and _ only");
  });

  it("does not submit anything: the form handler stops the default", () => {
    const app = form();
    const event = new Event("submit", { cancelable: true });
    app.querySelector("#launch-form")!.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });
});
