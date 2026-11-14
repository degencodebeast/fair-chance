import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { optionsFromConfig, scanBuildOutput } from "../scripts/scan-build-output.ts";

// The scan uses the real fixed config. The fixture holds the test accounts of our own testnet run.
let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "fc-own-"));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

const scan = async (names: string[]) => {
  writeFileSync(join(dir, "recorded-run.json"), JSON.stringify({ accounts: names }));
  return (await scanBuildOutput(dir, optionsFromConfig({}))).map((f) => f.rule);
};

describe("our own testnet test accounts in the recorded run file", () => {
  it("passes the buyer, creator, coin and cabal accounts of our run", async () => {
    expect(
      await scan([
        "bag-buyer-01.fairchance.testnet",
        "bag-buyer-20.fairchance.testnet",
        "bag-creator.fairchance.testnet",
        "cabal-wallet-03.fairchance.testnet",
        "bag.factory.fairchance.testnet",
        "factory.fairchance.testnet",
      ]),
    ).toEqual([]);
  });

  it.each([
    ["a real-looking account under another name", "alice-trader.testnet"],
    ["an account under another .near root", "bag-buyer-01.fairchance.near"],
    ["a buyer name under a wrong parent", "bag-buyer-01.evil.testnet"],
    ["a sub-account of one of our test accounts", "x.bag-buyer-01.fairchance.testnet"],
    ["a name that only starts like ours", "bag-buyer-01x.fairchance.testnet"],
    ["another name under our own root", "alice.fairchance.testnet"],
    ["our root with a prefix attached", "evilbag-buyer-01.fairchance.testnet"],
  ])("still fails %s", async (_n, name) => {
    expect(await scan([name])).toEqual(["wallet"]);
  });
});
