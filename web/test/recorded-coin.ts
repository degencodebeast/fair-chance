// A chain reader built from a recorded coin page state (testdata/coin-page-states/<state>.json).
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { parseLog, ViewNotAvailable, type AccountFacts, type ChainReader, type CoinEvent } from "../src/chain.ts";

export interface RecordedCoin {
  description: string;
  now_ns: string;
  views: Record<string, unknown>;
  missing_views: string[];
  events: { receipt_id: string; block: number; time_ns: string; line: string }[];
  account: { code_hash: string; global_contract_hash: string | null; global_contract_account_id: string | null };
  access_key_count: number;
  fail: string | null;
}

export const STATES = join(import.meta.dirname, "../../testdata/coin-page-states");

export function recordedCoin(state: string): RecordedCoin {
  return JSON.parse(readFileSync(join(STATES, `${state}.json`), "utf8")) as RecordedCoin;
}

export function readerFrom(rec: RecordedCoin, coin: string): ChainReader {
  const fail = () => {
    if (rec.fail !== null) throw new Error(rec.fail);
  };
  return {
    async view<T>(method: string): Promise<T> {
      fail();
      if (rec.missing_views.includes(method) || !(method in rec.views)) throw new ViewNotAvailable(method);
      return rec.views[method] as T;
    },
    async account(): Promise<AccountFacts> {
      fail();
      return {
        globalContractHash: rec.account.global_contract_hash,
        usesGlobalContractByAccount: rec.account.global_contract_account_id !== null,
        localCodeHash: rec.account.code_hash === "11111111111111111111111111111111" ? null : rec.account.code_hash,
      };
    },
    async accessKeyCount() {
      fail();
      return rec.access_key_count;
    },
    async events() {
      fail();
      const events: CoinEvent[] = rec.events.map((e) => {
        const parsed = parseLog(e.line);
        if (parsed === null) throw new Error(`recorded line is not an event: ${e.line}`);
        return { ...parsed, coin, receiptId: e.receipt_id, block: e.block, timeNs: e.time_ns };
      });
      return { events, cursor: 0 };
    },
  };
}
