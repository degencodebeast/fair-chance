import { describe, expect, it } from "vitest";
import { landingCopy } from "../src/landing-copy.ts";
import { allText, copyViolations } from "./copy-rules.ts";

describe("landing copy", () => {
  it("uses the builder's headline", () => {
    expect(landingCopy.headline).toBe("You buy. The creator dumps on you. You hold the bag.");
    expect(allText(landingCopy).join("\n")).not.toContain("The creator sells.");
  });

  it("keeps the rest of the hero text as designed", () => {
    expect(landingCopy.subheadline).toBe("On Fair Chance, they have to sell slowly.");
    expect(landingCopy.intro).toMatch(/^Every coin launched here has sell limits built into its contract\./);
  });

  it("follows the copy rules in every string", () => {
    for (const text of allText(landingCopy)) expect(copyViolations(text), text).toEqual([]);
  });

  it("labels the example receipt as sample data and names no real coin", () => {
    expect(landingCopy.proof.label).toMatch(/Sample data/);
    expect(landingCopy.proof.sentence).toMatch(/tried to sell 3%/);
  });

  it("states the known limits and keeps the rules claims to the documented ones", () => {
    expect(landingCopy.wont).toHaveLength(3);
    expect(landingCopy.wont[0]).toBe("It slows a dump. It doesn't stop one.");
    const claims = allText(landingCopy).join("\n");
    for (const banned of ["rug-proof", "cannot be rugged", "audited", "guaranteed"]) expect(claims).not.toContain(banned);
  });

  it("has no buttons that do nothing: every action names the screen it opens", () => {
    expect(landingCopy.actions.map((a) => a.opens)).toEqual(["coins", "launch-off", "rules"]);
  });
});
