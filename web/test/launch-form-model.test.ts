import { describe, expect, it } from "vitest";
import { LAUNCH_LIMITS, launchFormModel } from "../src/launch-form-model.ts";
import { allText, copyViolations } from "./copy-rules.ts";

const ok = { name: "Moth Lamp", ticker: "MOTH", description: "A lamp for moths. Nothing else.", openingBuy: "4.40" };

describe("launch form limits", () => {
  it("has the design's lengths in one place", () => {
    expect(LAUNCH_LIMITS).toEqual({ name: 32, ticker: 8, description: 80 });
  });
});

describe("launch form model", () => {
  it("counts the characters of each field", () => {
    const m = launchFormModel(ok);
    expect(m.counts).toEqual({ name: "9/32", ticker: "4/8", description: "31/80" });
    expect(m.errors).toEqual([]);
  });

  it("refuses a description above the limit and says how long it may be", () => {
    const m = launchFormModel({ ...ok, description: "x".repeat(81) });
    expect(m.errors).toContain("The description can be 80 characters at most.");
  });

  it("needs a name and a ticker of letters and digits", () => {
    expect(launchFormModel({ ...ok, name: "" }).errors).toContain("Type a name.");
    expect(launchFormModel({ ...ok, ticker: "" }).errors).toContain("Type a ticker.");
    expect(launchFormModel({ ...ok, ticker: "MO TH" }).errors).toContain("The ticker can use letters, digits, - and _ only.");
    expect(launchFormModel({ ...ok, ticker: "ABCDEFGHI" }).errors).toContain("The ticker can be 8 characters at most.");
  });

  it("accepts no opening buy, and refuses a buy that is not a number of NEAR", () => {
    expect(launchFormModel({ ...ok, openingBuy: "" }).errors).toEqual([]);
    expect(launchFormModel({ ...ok, openingBuy: "abc" }).errors).toContain("Type the opening buy as a number of NEAR, for example 4.40, or leave it empty.");
  });

  it("states the creator group limit in the opening buy help", () => {
    expect(launchFormModel(ok).buyHelp).toMatch(/4% of supply/);
    expect(launchFormModel(ok).buyHelp).toMatch(/early supply/);
  });

  it("turns the launch button off, valid or not, and says why", () => {
    for (const input of [ok, { ...ok, name: "" }]) {
      const m = launchFormModel(input);
      expect(m.button.reason).toMatch(/turned off/);
    }
    expect(launchFormModel(ok).button.label).toBe("Launch Moth Lamp");
    expect(launchFormModel({ ...ok, name: "" }).button.label).toBe("Launch coin");
  });

  it("shows the preview in capitals for the ticker", () => {
    expect(launchFormModel({ ...ok, ticker: "moth" }).preview).toMatchObject({ name: "Moth Lamp", ticker: "MOTH", description: ok.description });
  });

  it("follows the copy rules", () => {
    for (const t of allText(launchFormModel(ok))) expect(copyViolations(t), t).toEqual([]);
  });
});

describe("ticker rule (as the factory builds the coin account)", () => {
  it("lowers uppercase: Bag becomes bag", () => {
    const m = launchFormModel({ ...ok, ticker: "Bag" });
    expect(m.errors).toEqual([]);
    expect(m.accountLabel).toBe("bag");
  });

  it("allows a-z, 0-9, - and _ between letters or digits", () => {
    for (const t of ["bag", "b-a_g", "b1", "a"]) expect(launchFormModel({ ...ok, ticker: t }).errors, t).toEqual([]);
  });

  it("refuses $BAG with a reason", () => {
    const m = launchFormModel({ ...ok, ticker: "$BAG" });
    expect(m.errors).toContain("The ticker can use letters, digits, - and _ only.");
    expect(m.accountLabel).toBeNull();
  });

  it("refuses a dot, a leading or trailing separator and two separators in a row", () => {
    for (const t of ["a.b", "-bag", "bag_", "b--g", "b-_g"]) {
      expect(launchFormModel({ ...ok, ticker: t }).errors.length, t).toBeGreaterThan(0);
    }
  });
});
