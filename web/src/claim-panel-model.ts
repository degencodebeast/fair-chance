// The text of the creator fee claim panel. Pure. The claim button is turned off in this version:
// claiming needs a wallet, and nothing on this page signs. Refusal sentences come from the shared
// refusal text file, never from this file.
import type { Reading } from "./coin-page.ts";
import type { CreatorFees } from "./creator-fees.ts";
import { nearAmount, timeWithHint } from "./format.ts";
import { refusalText } from "./refusal-texts.ts";
import { NOT_AVAILABLE_FROM_COIN } from "./trade-box-model.ts";

const HOUR_NS = 3_600_000_000_000n;

export interface ClaimPanelInput {
  fees: Reading<CreatorFees>;
  /** `fc_rules.claim_min_real_near` and `fc_rules.claim_depth_hold_s`. */
  claimMinRealNear: bigint;
  claimDepthHoldS: number;
  nowNs: bigint;
  timeZone?: string;
}

export interface ClaimPanelModel {
  /** "0.420 NEAR"; null when the coin has no creator fee view. */
  amount: string | null;
  message: string;
  /** Who can claim. Null when the view is missing. */
  viewer: string | null;
  progress: { text: string; pct: number } | null;
  button: { label: string; reason: string };
  refusals: { notReady: string | null; notCreator: string | null };
}

const BUTTON_REASON =
  "Claiming is turned off in this version. The creator account claims with its own wallet, and this page cannot connect one.";

export function claimPanelModel(input: ClaimPanelInput): ClaimPanelModel {
  const refusals = { notReady: refusalText("FC_CLAIM_NOT_READY"), notCreator: refusalText("FC_NOT_CREATOR") };
  if (input.fees.kind === "unavailable") {
    return { amount: null, message: NOT_AVAILABLE_FROM_COIN, viewer: null, progress: null, button: { label: "Claim", reason: BUTTON_REASON }, refusals };
  }
  const f = input.fees.value;
  const amount = `${nearAmount(f.balance.toString(), 3)} NEAR`;
  const minimum = `${nearAmount(input.claimMinRealNear.toString(), 2)} real NEAR`;
  const holdHours = input.claimDepthHoldS / 3600;
  const ctx = { nowNs: input.nowNs, ...(input.timeZone ? { timeZone: input.timeZone } : {}) };
  let message: string;
  let progress: ClaimPanelModel["progress"] = null;
  if (f.claimableNow) {
    message = `${amount} is ready for the creator to claim.`;
  } else if (f.depthOkSinceNs === null) {
    message = `The pool is below ${minimum}. The ${holdHours} hours start again when it is back above.`;
  } else {
    const elapsed = input.nowNs > f.depthOkSinceNs ? input.nowNs - f.depthOkSinceNs : 0n;
    const heldHours = Number(elapsed / HOUR_NS);
    progress = { text: `${heldHours} h of ${holdHours}`, pct: Math.min(100, Math.floor((heldHours * 100) / holdHours)) };
    message = `Claimable after the pool holds at least ${minimum} for ${holdHours} h.`;
    if (f.claimableAtNs !== null) message += ` Earliest claim ${timeWithHint(f.claimableAtNs, ctx)}.`;
  }
  return {
    amount,
    message,
    viewer: `The creator account is ${f.creator}. That account can claim.`,
    progress,
    button: { label: `Claim ${amount}`, reason: BUTTON_REASON },
    refusals,
  };
}
