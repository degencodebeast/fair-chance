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

  it("asks for a wallet, then for valid fields, before the launch button turns on", () => {
    expect(launchFormModel(ok).button).toEqual({ label: "Launch Moth Lamp", reason: "Connect a wallet to launch." });
    expect(launchFormModel({ ...ok, name: "", signedIn: "you.testnet" }).button).toEqual({ label: "Launch coin", reason: "Fix the items above first." });
    expect(launchFormModel({ ...ok, signedIn: "you.testnet" }).button).toEqual({ label: "Launch Moth Lamp", reason: null });
  });

  it("turns the button off while the wallet is open", () => {
    expect(launchFormModel({ ...ok, signedIn: "you.testnet", pending: true }).button).toEqual({ label: "Waiting for the wallet", reason: "Approve or close the transaction in your wallet." });
  });

  it("says the total NEAR the launch attaches, and that the deposit stays in the coin account", () => {
    const m = launchFormModel({ ...ok, storageDeposit: 10n ** 24n });
    expect(m.deposit).toEqual({
      total: "You attach 5.4 NEAR: a 1 NEAR deposit and a 4.4 NEAR opening buy.",
      note: "The deposit stays in the coin account. Nobody can take it out.",
    });
    expect(launchFormModel({ ...ok, openingBuy: "", storageDeposit: 10n ** 24n }).deposit?.total).toBe("You attach 1 NEAR: the deposit.");
    expect(launchFormModel({ ...ok, openingBuy: "abc", storageDeposit: 10n ** 24n }).deposit?.total).toBe("You attach 1 NEAR for the deposit, plus the opening buy.");
  });

  it("shows the preview in capitals for the ticker", () => {
    expect(launchFormModel({ ...ok, ticker: "moth" }).preview).toMatchObject({ name: "Moth Lamp", ticker: "MOTH", description: ok.description });
  });

  it("follows the copy rules", () => {
    for (const t of allText(launchFormModel({ ...ok, storageDeposit: 10n ** 24n }))) expect(copyViolations(t), t).toEqual([]);
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

  it("refuses a letter outside ASCII, even one that lowers or uppers to ASCII (Kelvin sign, dotless i, sharp s)", () => {
    for (const t of ["K", "Kag", "ıab", "baß", "bаg"]) {
      const m = launchFormModel({ ...ok, ticker: t });
      expect(m.errors, t).toContain("The ticker can use letters, digits, - and _ only.");
      expect(m.accountLabel, t).toBeNull();
      expect(m.launchArgs, t).toBeNull();
    }
  });

  it("refuses a dot, a leading or trailing separator and two separators in a row", () => {
    for (const t of ["a.b", "-bag", "bag_", "b--g", "b-_g"]) {
      expect(launchFormModel({ ...ok, ticker: t }).errors.length, t).toBeGreaterThan(0);
    }
  });
});

describe("launch form picture", () => {
  const icon = "data:image/webp;base64,UklGRg==";

  it("passes the picture into the launch arguments as the factory takes them", () => {
    const m = launchFormModel({ ...ok, icon });
    expect(m.launchArgs).toEqual({
      name: "Moth Lamp",
      ticker: "MOTH",
      icon,
      description: "A lamp for moths. Nothing else.",
      opening_buy: "4400000000000000000000000",
    });
  });

  it("sends null for a missing picture, description and opening buy", () => {
    const m = launchFormModel({ name: "Moth Lamp", ticker: "MOTH", description: "", openingBuy: "" });
    expect(m.launchArgs).toEqual({ name: "Moth Lamp", ticker: "MOTH", icon: null, description: null, opening_buy: null });
  });

  it("gives no launch arguments while a field has an error", () => {
    expect(launchFormModel({ ...ok, icon, name: "" }).launchArgs).toBeNull();
  });

  it("refuses a picture the coin would refuse", () => {
    const m = launchFormModel({ ...ok, icon: "data:image/png;base64,AAAA" });
    expect(m.errors).toContain("The picture must be a WebP image under 8 KB. Choose it again.");
    expect(m.launchArgs).toBeNull();
  });
});

describe("launch form picture state", () => {
  it("gives no launch arguments while the picture is being shrunk", () => {
    const m = launchFormModel({ ...ok, picture: { state: "pending" } });
    expect(m.launchArgs).toBeNull();
    expect(m.errors).toContain("The picture is still being shrunk. Wait for it.");
  });

  it("gives no launch arguments and shows the refusal when the picture was refused", () => {
    const m = launchFormModel({ ...ok, picture: { state: "refused", reason: "This picture does not fit in 8 KB, even at 64 px. Choose a simpler picture." } });
    expect(m.launchArgs).toBeNull();
    expect(m.errors).toContain("This picture does not fit in 8 KB, even at 64 px. Choose a simpler picture.");
  });

  it("gives valid arguments with no icon when no file is chosen", () => {
    expect(launchFormModel({ ...ok, picture: { state: "none" } }).launchArgs?.icon).toBeNull();
  });
});
