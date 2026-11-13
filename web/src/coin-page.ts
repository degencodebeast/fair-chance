// Loads everything the coin page shows, through the chain-read module only. The result is either
// all of it or an error: when any read fails, no number from an earlier or partial read is kept.
import { chainReader, configFromUrl, ViewNotAvailable, type ChainReader, type CoinEvent } from "./chain.ts";
import { runCoinChecks, type CheckResult } from "./chain-checks.ts";
import { knownDeploymentFor } from "./known-deployments.ts";
import {
  parseCap,
  parseEarlyWindow,
  parseMetadata,
  parsePool,
  parseRules,
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
  rules: Rules;
  pool: Reading<Pool>;
  earlyWindow: Reading<EarlyWindow>;
  cap: Reading<Cap>;
  totalSupply: Reading<bigint>;
  metadata: Reading<TokenMetadata>;
  events: CoinEvent[];
  checks: CheckResult[];
}

export type CoinPageData =
  | { kind: "ready"; reads: CoinReads; highlightReceipt: string | null }
  | { kind: "error"; message: string; attemptedAtNs: bigint };

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
    const cfg = configFromUrl(href);
    const reader = load.reader ?? chainReader(cfg, load.fetchFn);
    const rules = parseRules(await reader.view("fc_rules"));
    const [pool, earlyWindow, cap, totalSupply, metadata, checks, scan] = await Promise.all([
      optional(() => reader.view("fc_pool"), parsePool),
      optional(() => reader.view("fc_early_window"), parseEarlyWindow),
      optional(() => reader.view("fc_cap"), parseCap),
      optional(() => reader.view("ft_total_supply"), parseTotalSupply),
      optional(() => reader.view("ft_metadata"), parseMetadata),
      runCoinChecks(reader, cfg.coin, knownDeploymentFor(href)),
      reader.events(rules.launchBlock),
    ]);
    const reads: CoinReads = { coin: cfg.coin, rules, pool, earlyWindow, cap, totalSupply, metadata, events: scan.events, checks };
    return { kind: "ready", reads, highlightReceipt: new URL(href).searchParams.get("receipt") };
  } catch (e) {
    return { kind: "error", message: e instanceof Error ? e.message : String(e), attemptedAtNs: load.nowNs };
  }
}
