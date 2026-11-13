// Typed readings of the coin's view methods. Each parser takes the JSON the coin returned (unknown),
// checks the fields the page uses once, at the boundary, and returns a named type.

export class UnexpectedView extends Error {}

type Fields = Record<string, unknown>;

function fields(value: unknown, view: string): Fields {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new UnexpectedView(`${view} did not return an object`);
  }
  return value as Fields;
}

function reader(view: string, f: Fields) {
  const bad = (key: string, want: string): never => {
    throw new UnexpectedView(`${view}.${key} is not ${want}`);
  };
  return {
    amount(key: string): bigint {
      const v = f[key];
      return typeof v === "string" && /^\d+$/.test(v) ? BigInt(v) : bad(key, "a decimal string");
    },
    count(key: string): number {
      const v = f[key];
      return typeof v === "number" && Number.isSafeInteger(v) && v >= 0 ? v : bad(key, "a whole number");
    },
    optionalCount(key: string): number | null {
      return f[key] === null || f[key] === undefined ? null : this.count(key);
    },
    flag(key: string): boolean {
      const v = f[key];
      return typeof v === "boolean" ? v : bad(key, "true or false");
    },
    text(key: string): string {
      const v = f[key];
      return typeof v === "string" ? v : bad(key, "text");
    },
    optionalText(key: string): string | null {
      return f[key] === null || f[key] === undefined ? null : this.text(key);
    },
  };
}

export interface Rules {
  creator: string;
  launchBlock: number;
  launchNs: bigint;
  windowUnmarkedS: number;
  windowEarlyS: number;
  earlyBlocks: number;
  largeThresholdBps: number;
  creatorCapBps: number;
  realCapCBps: number;
  floorBps: number;
  penaltyBps: number;
  virtualNear: bigint;
  swapFeeBps: number;
  creatorFeeShareBps: number;
  claimMinRealNear: bigint;
  claimDepthHoldS: number;
  unclaimedToPoolS: number | null;
  codeHash: string;
  description: string | null;
}

export function parseRules(value: unknown): Rules {
  const r = reader("fc_rules", fields(value, "fc_rules"));
  return {
    creator: r.text("creator"),
    launchBlock: r.count("launch_block"),
    launchNs: r.amount("launch_ns"),
    windowUnmarkedS: r.count("window_unmarked_s"),
    windowEarlyS: r.count("window_early_s"),
    earlyBlocks: r.count("early_blocks"),
    largeThresholdBps: r.count("large_threshold_bps"),
    creatorCapBps: r.count("creator_cap_bps"),
    realCapCBps: r.count("real_cap_c_bps"),
    floorBps: r.count("floor_bps"),
    penaltyBps: r.count("penalty_bps"),
    virtualNear: r.amount("virtual_near"),
    swapFeeBps: r.count("swap_fee_bps"),
    creatorFeeShareBps: r.count("creator_fee_share_bps"),
    claimMinRealNear: r.amount("claim_min_real_near"),
    claimDepthHoldS: r.count("claim_depth_hold_s"),
    unclaimedToPoolS: r.optionalCount("unclaimed_to_pool_s"),
    codeHash: r.text("code_hash"),
    description: r.optionalText("description"),
  };
}

export interface Pool {
  tokenReserve: bigint;
  nearReserve: bigint;
  virtualNear: bigint;
  realNear: bigint;
  creatorFeeBalance: bigint;
  largeHolders: number;
  creatorGroupShareBps: number;
  block: number;
}

export function parsePool(value: unknown): Pool {
  const r = reader("fc_pool", fields(value, "fc_pool"));
  return {
    tokenReserve: r.amount("token_reserve"),
    nearReserve: r.amount("near_reserve"),
    virtualNear: r.amount("virtual_near"),
    realNear: r.amount("real_near"),
    creatorFeeBalance: r.amount("creator_fee_balance"),
    largeHolders: r.count("large_holders"),
    creatorGroupShareBps: r.count("creator_group_share_bps"),
    block: r.count("block"),
  };
}

export interface EarlyWindow {
  earlyBlocks: number;
  blocksLeft: number;
  earlySupplyTotal: bigint;
  sharedAllowance: bigint;
  buyFeeBpsNow: number;
  feeEndBlock: number;
}

export function parseEarlyWindow(value: unknown): EarlyWindow {
  const r = reader("fc_early_window", fields(value, "fc_early_window"));
  return {
    earlyBlocks: r.count("early_blocks"),
    blocksLeft: r.count("blocks_left"),
    earlySupplyTotal: r.amount("early_supply_total"),
    sharedAllowance: r.amount("shared_allowance"),
    buyFeeBpsNow: r.count("buy_fee_bps_now"),
    feeEndBlock: r.count("fee_end_block"),
  };
}

export interface Cap {
  windowStartNs: bigint;
  windowEndNs: bigint;
  capTotal: bigint;
  capUsed: bigint;
  capLeft: bigint;
  isFull: boolean;
}

export function parseCap(value: unknown): Cap {
  const r = reader("fc_cap", fields(value, "fc_cap"));
  return {
    windowStartNs: r.amount("window_start_ns"),
    windowEndNs: r.amount("window_end_ns"),
    capTotal: r.amount("cap_total"),
    capUsed: r.amount("cap_used"),
    capLeft: r.amount("cap_left"),
    isFull: r.flag("is_full"),
  };
}

export interface TokenMetadata {
  name: string;
  symbol: string;
  decimals: number;
  icon: string | null;
}

export function parseMetadata(value: unknown): TokenMetadata {
  const r = reader("ft_metadata", fields(value, "ft_metadata"));
  return { name: r.text("name"), symbol: r.text("symbol"), decimals: r.count("decimals"), icon: r.optionalText("icon") };
}

export function parseTotalSupply(value: unknown): bigint {
  if (typeof value !== "string" || !/^\d+$/.test(value)) throw new UnexpectedView("ft_total_supply is not a decimal string");
  return BigInt(value);
}
