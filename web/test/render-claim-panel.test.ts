// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { parseCreatorFees } from "../src/creator-fees.ts";
import { claimPanelModel } from "../src/claim-panel-model.ts";
import { drawClaimPanel, drawCreatorFees } from "../src/render-claim-panel.ts";
import { walletControl, type WalletControl } from "../src/wallet-session.ts";
import { copyViolations } from "./copy-rules.ts";
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
    expect(reasons["claim-fees-button"]).toMatch(/cannot claim yet/);
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

const PAGE = "https://example.test/?network=testnet&coin=moth.factory.testnet";
function walletOf(accountId: string | null): WalletControl {
  return walletControl(PAGE, () => ({
    connect: async () => ({ accountId: accountId ?? "x.testnet", network: "testnet" }),
    restore: async () => null,
    send: async () => [],
    signOut: async () => {},
    onSignIn: () => {},
    onSignOut: () => {},
  }));
}

describe("creator fees for the creator and for everyone else", () => {
  const model = claimPanelModel({ fees: { kind: "value", value: fees }, ...rules });

  it("shows the short line, and no panel, with no wallet", () => {
    const { panel, line } = drawCreatorFees(model, walletOf(null));
    document.body.replaceChildren(panel!, line!);
    expect(line!.textContent).toBe("Creator fees: 0.420 NEAR, held for lamp.testnet.");
    expect(copyViolations(line!.textContent ?? "")).toEqual([]);
    expect(line!.hidden).toBe(false);
    expect(panel!.hidden).toBe(true);
  });

  it("shows the short line for a wallet that is not the creator", async () => {
    const w = walletOf("other.testnet");
    const { panel, line } = drawCreatorFees(model, w);
    document.body.replaceChildren(panel!, line!);
    await w.connect();
    expect(panel!.hidden).toBe(true);
    expect(line!.hidden).toBe(false);
  });

  it("shows the full panel, with the claim button off, when the wallet is the creator, and follows the wallet", async () => {
    const w = walletOf("lamp.testnet");
    const { panel, line } = drawCreatorFees(model, w);
    document.body.replaceChildren(panel!, line!);
    await w.connect();
    expect(panel!.hidden).toBe(false);
    expect(line!.hidden).toBe(true);
    expect(turnedOffReasons(panel!)["claim-fees-button"]).toMatch(/cannot claim yet/);
    await w.signOut();
    expect(panel!.hidden).toBe(true);
    expect(line!.hidden).toBe(false);
  });

  it("shows nothing when the coin has no creator fee view", () => {
    const none = claimPanelModel({ fees: { kind: "unavailable" }, ...rules });
    expect(drawCreatorFees(none, walletOf(null))).toEqual({ panel: null, line: null });
  });
});
