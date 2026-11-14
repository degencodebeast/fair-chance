import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { landingCopy } from "../src/landing-copy.ts";
import { allText, copyViolations } from "./copy-rules.ts";

describe("landing copy", () => {
  it("uses the builder's headline", () => {
    expect(landingCopy.headline).toBe("You buy. The cabal dumps on you. You're left holding the bag.");
    expect(allText(landingCopy).join("\n")).not.toContain("The creator sells.");
  });

  it("keeps the rest of the hero text as designed", () => {
    expect(landingCopy.intro).toMatch(/^Every coin launched here has sell limits built into its contract\./);
  });

  it("puts the pitch line under the headline, with 'slows' and without 'stops' or 'protects'", () => {
    expect(landingCopy.subheadline).toBe("Fair Chance slows the dump at launch, inside the coin contract.");
    expect(landingCopy.subheadline).toMatch(/\bslows\b/);
    expect(landingCopy.subheadline).not.toMatch(/\b(stops?|protects?)\b/i);
    expect(copyViolations(landingCopy.subheadline)).toEqual([]);
  });

  it("says small bags are not limited, with the one exception at a full cap", () => {
    const item = landingCopy.buyers.items.find((i) => i.b === "Small bags are not limited.");
    expect(item?.t).toBe("One exception: when the pool cap has too little room, profit on tokens bought in the same hour is held back.");
  });

  it("does not call a late splitter unlimited without the same exception", () => {
    expect(landingCopy.wont[2]).toBe("Someone who buys later through many small wallets is not limited, except that profit on tokens bought in the same hour is held back when the pool cap has too little room.");
  });

  it("follows the copy rules in every string", () => {
    for (const text of allText(landingCopy)) expect(copyViolations(text), text).toEqual([]);
  });

  it("shows the real CABAL2 testnet result with its source, no sample data", () => {
    expect(landingCopy.proof.source).toBe("From the Cabal Test 2 coin · 14 Nov, 10:12 UTC · NEAR testnet");
    expect(landingCopy.proof.sentence).toBe("Our test cabal of 10 wallets bought within 8 seconds of launch. Together they held 3.95% of the supply. Then they tried to sell all 39.5M CABAL2 at once. 2.98M sold, 7.5% of what they asked for. The other 36.6M stayed in their wallets, because the sell limit slows early buyers.");
    expect(landingCopy.proof.coin).toBe("cabal2.factory.fairchance.testnet");
  });

  it("states the known limits and keeps the rules claims to the documented ones", () => {
    expect(landingCopy.wont).toHaveLength(3);
    expect(landingCopy.wont[0]).toBe("It slows a dump. It doesn't stop one.");
    const claims = allText(landingCopy).join("\n");
    for (const banned of ["rug-proof", "cannot be rugged", "audited", "guaranteed"]) expect(claims).not.toContain(banned);
  });

  it("has no buttons that do nothing: every action names the screen it opens", () => {
    expect(landingCopy.actions.map((a) => a.opens)).toEqual(["demo", "launch", "rules"]);
  });
});

describe("hero buttons", () => {
  it("says Watch the demo for the video and Open app until the video link is set", () => {
    expect(landingCopy.actions[0].label).toBe("Watch the demo");
    expect(landingCopy.openAppLabel).toBe("Open app");
    expect(landingCopy.demoUrl === null || landingCopy.demoUrl.startsWith("https://")).toBe(true);
  });
});

describe("README pitch line", () => {
  it("is its own plain paragraph right after the bold headline", () => {
    const lines = readFileSync(new URL("../../README.md", import.meta.url), "utf8").split("\n");
    const head = lines.indexOf(`**${landingCopy.headline}**`);
    expect(head).toBeGreaterThan(-1);
    expect(lines[head + 1]).toBe("");
    expect(lines[head + 2]).toBe(
      "The coin cannot know who is in a cabal, so it gives a tighter limit to whoever bought in the first blocks.",
    );
    expect(lines[head + 3]).toBe("");
    expect(lines[head + 4]).toBe(landingCopy.subheadline);
    expect(lines[head + 5]).toBe("");
  });
});
