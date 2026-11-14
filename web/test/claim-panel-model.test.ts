import { describe, expect, it } from "vitest";
import { parseCreatorFees } from "../src/creator-fees.ts";
import { claimPanelModel } from "../src/claim-panel-model.ts";
import { refusalText } from "../src/refusal-texts.ts";
import { allText, copyViolations } from "./copy-rules.ts";

const HOUR = 3_600_000_000_000n;
const now = 1_800_000_000_000_000_000n;
const view = {
  creator: "lamp.testnet",
  balance: "420000000000000000000000",
  claimed_total: "0",
  depth_ok_since_ns: (now - 18n * HOUR).toString(),
  claimable_now: false,
  claimable_at_ns: (now + 6n * HOUR).toString(),
};
const rules = { claimMinRealNear: 20n * 10n ** 24n, claimDepthHoldS: 86_400 };
const model = (over: object = {}) => claimPanelModel({ fees: { kind: "value", value: parseCreatorFees({ ...view, ...over }) }, ...rules, nowNs: now, timeZone: "UTC" });

describe("fc_creator_fees", () => {
  it("reads the view and keeps the empty times as null", () => {
    expect(parseCreatorFees({ ...view, depth_ok_since_ns: null, claimable_at_ns: null })).toMatchObject({
      creator: "lamp.testnet",
      balance: 420_000_000_000_000_000_000_000n,
      depthOkSinceNs: null,
      claimableAtNs: null,
      claimableNow: false,
    });
  });
});

describe("claim panel", () => {
  it("shows the balance and the 24 hour progress from the view", () => {
    const m = model();
    expect(m.amount).toBe("0.420 NEAR");
    expect(m.progress).toEqual({ text: "18 h of 24", pct: 75 });
    expect(m.message).toMatch(/at least 20\.00 real NEAR for 24 h/);
    expect(m.message).toMatch(/Earliest claim/);
  });

  it("reads ready when the view says claimable_now", () => {
    const m = model({ claimable_now: true, claimable_at_ns: null });
    expect(m.message).toBe("0.420 NEAR is ready for the creator to claim.");
    expect(m.progress).toBeNull();
  });

  it("says the pool is below the minimum when no depth time is counting", () => {
    const m = model({ depth_ok_since_ns: null, claimable_at_ns: null });
    expect(m.message).toBe("The pool is below 20.00 real NEAR. The 24 hours start again when it is back above.");
  });

  it("turns the claim button off and says why, naming the creator", () => {
    const m = model();
    expect(m.button.label).toBe("Claim 0.420 NEAR");
    expect(m.button.reason).toMatch(/cannot claim yet/);
    expect(m.viewer).toBe("The creator account is lamp.testnet. Only that account can claim.".replace("Only that account", "That account"));
  });

  it("takes the refusal sentences from the shared file", () => {
    const m = model();
    expect(m.refusals.notReady).toBe(refusalText("FC_CLAIM_NOT_READY"));
    expect(m.refusals.notCreator).toBe(refusalText("FC_NOT_CREATOR"));
  });

  it("says so when the coin has no creator fee view", () => {
    const m = claimPanelModel({ fees: { kind: "unavailable" }, ...rules, nowNs: now, timeZone: "UTC" });
    expect(m.amount).toBeNull();
    expect(m.message).toBe("Not available from this coin");
    expect(m.button.reason).toMatch(/cannot claim yet/);
  });

  it("follows the copy rules", () => {
    for (const t of allText(model())) expect(copyViolations(t), t).toEqual([]);
  });
});
