// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { parseCreatorFees } from "../src/creator-fees.ts";
import { claimPanelModel } from "../src/claim-panel-model.ts";
import { drawClaimPanel } from "../src/render-claim-panel.ts";
import { expectNoEnabledControlWithoutHandler, turnedOffReasons } from "./dom-controls.ts";

const HOUR = 3_600_000_000_000n;
const now = 1_800_000_000_000_000_000n;
const fees = parseCreatorFees({
  creator: "lamp.testnet",
  balance: "420000000000000000000000",
  claimed_total: "0",
  depth_ok_since_ns: (now - 18n * HOUR).toString(),
  claimable_now: true,
  claimable_at_ns: null,
});
const rules = { claimMinRealNear: 20n * 10n ** 24n, claimDepthHoldS: 86_400, nowNs: now, timeZone: "UTC" };

describe("claim panel", () => {
  it("draws the balance and the message, and turns the claim button off with its reason", () => {
    const root = drawClaimPanel(claimPanelModel({ fees: { kind: "value", value: fees }, ...rules }));
    document.body.replaceChildren(root);
    expect(root.id).toBe("creator-fees");
    expect(root.querySelector("#creator-fees-amount")?.textContent).toBe("0.420 NEAR");
    expect(root.textContent).toContain("is ready for the creator to claim.");
    const reasons = turnedOffReasons(root);
    expect(Object.keys(reasons)).toEqual(["claim-fees-button"]);
    expect(reasons["claim-fees-button"]).toMatch(/turned off/);
    expect(root.querySelector<HTMLButtonElement>("#claim-fees-button")!.textContent).toBe("Claim 0.420 NEAR");
    expectNoEnabledControlWithoutHandler(root);
  });

  it("draws the progress bar while the 24 hours count", () => {
    const counting = parseCreatorFees({ ...{ creator: "lamp.testnet", balance: "1", claimed_total: "0", claimable_now: false }, depth_ok_since_ns: (now - 18n * HOUR).toString(), claimable_at_ns: null });
    const root = drawClaimPanel(claimPanelModel({ fees: { kind: "value", value: counting }, ...rules }));
    expect(root.querySelector("#creator-fees-progress")?.textContent).toBe("18 h of 24");
    expect(root.querySelector<HTMLElement>(".meter-used")?.style.width).toBe("75%");
  });

  it("draws the missing-view text and still turns the button off", () => {
    const root = drawClaimPanel(claimPanelModel({ fees: { kind: "unavailable" }, ...rules }));
    expect(root.textContent).toContain("Not available from this coin");
    turnedOffReasons(root);
  });
});
