// Loads everything the coin page shows, through the chain-read module only. The result is either
// all of it or an error: when any read fails, no number from an earlier or partial read is kept.
import { chainReader, configFromUrl, eventsFromLaunch, ViewNotAvailable, type ChainReader, type CoinEvent } from "./chain.ts";
import { HttpStatusError } from "./refresher.ts";
import { parseCreatorFees, type CreatorFees } from "./creator-fees.ts";
import { runCoinChecks, type CheckResult } from "./chain-checks.ts";
import { resolvePageLink, sourceLabel } from "./page-link.ts";
import { pageRpcRoute } from "./rpc-route.ts";
import {
  parseCap,
  parseEarlyWindow,
  parseMetadata,
  parsePool,
  parseRules,
  parseStorageMin,
  parseTotalSupply,
  type Cap,
  type EarlyWindow,
  type Pool,
  type Rules,
  type TokenMetadata,
} from "./coin-views.ts";

/** A view the coin returned, or one it does not have. */
export type Reading<T> = { kind: "value"; value: T } | { kind: "unavailable" };

export interface CoinReads {
  coin: string;
  /** The RPC host (and port) the checks and the reads used. */
  rpcHost: string;
  /** The time the reads were made, for the "as of" label. */
  loadedAtNs: bigint;
  rules: Rules;
  pool: Reading<Pool>;
  earlyWindow: Reading<EarlyWindow>;
  cap: Reading<Cap>;
  creatorFees: Reading<CreatorFees>;
  totalSupply: Reading<bigint>;
  /** `storage_balance_bounds().min`, for the buy reason. */
  storageMin: Reading<bigint>;
  metadata: Reading<TokenMetadata>;
  events: CoinEvent[];
  checks: CheckResult[];
}

export type CoinPageData =
  | { kind: "ready"; reads: CoinReads; highlightReceipt: string | null }
  | { kind: "error"; message: string; attemptedAtNs: bigint; source: string; /** Set when the read failed with an HTTP status, so a refresh can back off. */ failure?: HttpStatusError };

async function optional<T>(read: () => Promise<unknown>, parse: (value: unknown) => T): Promise<Reading<T>> {
  try {
    return { kind: "value", value: parse(await read()) };
  } catch (e) {
    if (e instanceof ViewNotAvailable) return { kind: "unavailable" };
    throw e;
  }
}

export interface PageLoad {
  fetchFn?: typeof fetch;
  /** Tests give a reader built from recorded view and event JSON. */
  reader?: ChainReader;
  nowNs: bigint;
}

export async function loadCoinPage(href: string, load: PageLoad): Promise<CoinPageData> {
  try {
    const link = resolvePageLink(href);
    const cfg = configFromUrl(href);
    const reader = load.reader ?? chainReader(cfg, load.fetchFn);
    const rules = parseRules(await reader.view("fc_rules"));
    const [pool, earlyWindow, cap, creatorFees, storageMin, totalSupply, metadata, checks, scan] = await Promise.all([
      optional(() => reader.view("fc_pool"), parsePool),
      optional(() => reader.view("fc_early_window"), parseEarlyWindow),
      optional(() => reader.view("fc_cap"), parseCap),
      optional(() => reader.view("fc_creator_fees"), parseCreatorFees),
      optional(() => reader.view("storage_balance_bounds"), parseStorageMin),
      optional(() => reader.view("ft_total_supply"), parseTotalSupply),
      optional(() => reader.view("ft_metadata"), parseMetadata),
      runCoinChecks(reader, cfg.coin, link.known),
      eventsFromLaunch(reader, rules.launchBlock),
    ]);
    const reads: CoinReads = { coin: cfg.coin, rpcHost: pageRpcRoute.readHost(link.rpcUrl, link.rpcHost), loadedAtNs: load.nowNs, rules, pool, earlyWindow, cap, creatorFees, storageMin, totalSupply, metadata, events: scan.events, checks };
    return { kind: "ready", reads, highlightReceipt: new URL(href).searchParams.get("receipt") };
  } catch (e) {
    return { kind: "error", message: e instanceof Error ? e.message : String(e), attemptedAtNs: load.nowNs, source: sourceLabel(href), ...(e instanceof HttpStatusError ? { failure: e } : {}) };
  }
}
