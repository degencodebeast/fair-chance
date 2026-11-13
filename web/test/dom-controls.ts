// Shared checks for the turned-off screens: a control the page leaves turned on must have a handler,
// and a control that is turned off must say why. A handler is added only through `listen`, which
// marks the element with data-handler.
import { expect } from "vitest";

const CONTROLS = "button, input, select, textarea, a[href]";

function hasHandler(node: Element): boolean {
  if (node.hasAttribute("data-handler")) return true;
  // A submit button works through the handler of its form.
  const form = (node as HTMLButtonElement).form;
  return node.getAttribute("type") === "submit" && form !== null && form !== undefined && form.hasAttribute("data-handler");
}

/** Every control that is not turned off has a handler. A plain link is not a control that acts. */
export function expectNoEnabledControlWithoutHandler(root: ParentNode): void {
  const missing = [...root.querySelectorAll(CONTROLS)]
    .filter((n) => !(n as HTMLButtonElement).disabled && n.tagName !== "A" && n.getAttribute("type") !== "hidden")
    .filter((n) => !hasHandler(n))
    .map((n) => `${n.tagName.toLowerCase()}#${n.id}`);
  expect(missing, "enabled controls with no handler").toEqual([]);
}

/** Every turned-off button names its reason in an element it points at. */
export function turnedOffReasons(root: ParentNode): Record<string, string> {
  const out: Record<string, string> = {};
  for (const b of root.querySelectorAll<HTMLButtonElement>("button[disabled]")) {
    const target = b.getAttribute("aria-describedby");
    const text = (target ? root.querySelector(`#${target}`)?.textContent : null) ?? "";
    expect(text.trim().length, `reason of button #${b.id}`).toBeGreaterThan(10);
    out[b.id] = text.trim();
  }
  return out;
}
