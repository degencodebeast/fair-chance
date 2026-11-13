// Typed reading of fc_creator_fees: the unclaimed creator balance and the claim progress.
import { fields, reader } from "./coin-views.ts";

export interface CreatorFees {
  creator: string;
  balance: bigint;
  claimedTotal: bigint;
  /** Null when the lowest real NEAR of the current window is below the minimum. */
  depthOkSinceNs: bigint | null;
  claimableNow: boolean;
  /** The earliest claim time if real NEAR stays at least the minimum; null when not counting. */
  claimableAtNs: bigint | null;
}

export function parseCreatorFees(value: unknown): CreatorFees {
  const f = fields(value, "fc_creator_fees");
  const r = reader("fc_creator_fees", f);
  const optionalAmount = (key: string): bigint | null => (f[key] === null || f[key] === undefined ? null : r.amount(key));
  return {
    creator: r.text("creator"),
    balance: r.amount("balance"),
    claimedTotal: r.amount("claimed_total"),
    depthOkSinceNs: optionalAmount("depth_ok_since_ns"),
    claimableNow: r.flag("claimable_now"),
    claimableAtNs: optionalAmount("claimable_at_ns"),
  };
}
