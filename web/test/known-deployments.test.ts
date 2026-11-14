import { describe, expect, it } from "vitest";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";

// Each value below was read from the testnet chain with a view call (see the commit message).
describe("the fixed testnet entry", () => {
  const t = KNOWN_NETWORKS.testnet;
  it("names the factory, the coin code hash and the demo coin the chain holds", () => {
    expect(t.factory).toBe("factory.fairchance.testnet");
    expect(t.codeHash).toBe("6Ld5DFTJ1BYc6uzb6YYvdK3yAMHdfBsvrwNEhsKE9er3");
    expect(t.demoCoin).toBe("bag.factory.fairchance.testnet");
  });
  it("reads factory events from the block of the first launch", () => {
    expect(t.factoryFromBlock).toBe(277565789);
  });
  it("serves the recorded run from a same-site file", () => {
    expect(t.recordedRunFile).toBe("recorded-run.json");
  });
  it("keeps the demo coin a direct sub-account of the factory", () => {
    expect(t.demoCoin.endsWith(`.${t.factory}`)).toBe(true);
    expect(t.demoCoin.slice(0, -t.factory.length - 1)).not.toContain(".");
  });
});
