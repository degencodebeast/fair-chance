// The buy and sell steps on fake chain reads and a fake wallet that returns real testnet outcomes.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { parseBuyQuote } from "../src/buy-quote.ts";
import { chainReader } from "../src/chain.ts";
import { loadSellLimit, sendBuy, sendLaunch, sendSell } from "../src/trade-actions.ts";
import type { FinalOutcome, TxRequest, WalletSession } from "../src/tx-types.ts";

const fixture = (name: string) => JSON.parse(readFileSync(join(import.meta.dirname, "fixtures/tx-outcomes", `${name}.json`), "utf8")) as FinalOutcome;

const COIN = "bag.factory.fairchance.testnet";
const BUYER = "bag-buyer-01.fairchance.testnet";

const buyQuote = {
  tokens_out: "7308624762134838070425063",
  fee_near: "10000000000000000000000",
  creator_fee_near: "3000000000000000000000",
  marked_tokens: "0",
  class_after: "small",
  becomes_large: false,
  allowance_after: "7100000000000000000000000",
};

function fakeChain(views: Record<string, unknown>) {
  const calls: [string, unknown][] = [];
  return {
    calls,
    async view<T>(method: string, args?: unknown): Promise<T> {
      calls.push([method, args]);
      if (!(method in views)) throw new Error(`no view ${method}`);
      const v = views[method];
      if (v instanceof Error) throw v;
      return (typeof v === "function" ? v() : v) as T;
    },
  };
}

function fakeSession(answer: () => Promise<FinalOutcome[]>, accountId = BUYER): WalletSession & { sent: TxRequest[][] } {
  const sent: TxRequest[][] = [];
  return {
    accountId,
    sent,
    async send(txs) {
      sent.push(txs);
      return answer();
    },
    signOut: async () => {},
  };
}

const ONE_NEAR = 1_000_000_000_000_000_000_000_000n;
/** The buy the visitor previewed: the NEAR amount and the quote the button shows. */
const previewed = (nearIn = ONE_NEAR) => ({ nearIn, quote: parseBuyQuote(buyQuote) });

describe("a buy", () => {
  it("builds from the previewed quote: the previewed NEAR, and min_tokens_out from the previewed tokens_out minus 1%", async () => {
    const chain = fakeChain({ storage_balance_of: null, storage_balance_bounds: { min: "1250000000000000000000", max: null } });
    const session = fakeSession(async () => [fixture("bag-buy")]);
    const out = await sendBuy({ chain, session, coin: COIN }, previewed());
    // No second quote: the buy sends what the button showed.
    expect(chain.calls.map((c) => c[0])).not.toContain("fc_quote_buy");
    expect(session.sent).toHaveLength(1);
    const [tx] = session.sent[0]!;
    expect(tx!.receiverId).toBe(COIN);
    expect(tx!.actions.map((a) => a.params.methodName)).toEqual(["storage_deposit", "buy"]);
    expect(tx!.actions[0]!.params.deposit).toBe("1250000000000000000000");
    expect(tx!.actions[1]!.params.args).toEqual({ min_tokens_out: "7235538514513489689720812" });
    expect(tx!.actions[1]!.params.deposit).toBe("1000000000000000000000000");
    expect(out).toMatchObject({ kind: "result", result: { kind: "bought", tokensOut: 7308624762134838070425063n } });
  });

  it("sends only the buy when the account already has a storage record", async () => {
    const chain = fakeChain({ storage_balance_of: { total: "1250000000000000000000", available: "0" } });
    const session = fakeSession(async () => [fixture("bag-buy")]);
    await sendBuy({ chain, session, coin: COIN }, previewed());
    expect(session.sent[0]![0]!.actions.map((a) => a.params.methodName)).toEqual(["buy"]);
  });

  it("says in one sentence that nothing was sent when the visitor closes the wallet", async () => {
    const chain = fakeChain({ storage_balance_of: { total: "1", available: "0" } });
    const session = fakeSession(async () => {
      throw new Error("User rejected");
    });
    expect(await sendBuy({ chain, session, coin: COIN }, previewed())).toEqual({ kind: "not-sent", text: "The wallet closed and sent nothing." });
  });

  it("reads only the two exact close messages of the wallet library as nothing sent", async () => {
    const outcome = async (e: unknown) =>
      (await sendBuy({ chain: fakeChain({ storage_balance_of: { total: "1", available: "0" } }), session: fakeSession(async () => { throw e; }), coin: COIN }, previewed())).kind;
    expect(await outcome(new Error("Wallet closed"))).toBe("not-sent");
    expect(await outcome(new Error("User rejected"))).toBe("not-sent");
    for (const other of ["User rejected the request", "Connection closed", "wallet closed", "Request cancelled"]) expect(await outcome(new Error(other)), other).toBe("unknown");
    expect(await outcome("Wallet closed")).toBe("unknown");
  });

  it("does not claim nothing was sent when the wallet fails in another way", async () => {
    const chain = fakeChain({ storage_balance_of: { total: "1", available: "0" } });
    const session = fakeSession(async () => {
      throw new Error("RPC timeout");
    });
    const out = await sendBuy({ chain, session, coin: COIN }, previewed());
    expect(out.kind).toBe("unknown");
    expect(out.kind === "unknown" && out.text).toBe("The wallet gave no result (RPC timeout). Check your wallet activity before you try again.");
  });

  it("sends nothing when the storage record cannot be read", async () => {
    const chain = fakeChain({ storage_balance_of: new Error("network down") });
    const session = fakeSession(async () => []);
    const out = await sendBuy({ chain, session, coin: COIN }, previewed());
    expect(session.sent).toHaveLength(0);
    expect(out).toEqual({ kind: "not-sent", text: "The storage record could not load, so nothing was sent. network down" });
  });
});

describe("the reads before a buy", () => {
  it("reads the storage record again after a request with no readable answer, and asks the wallet once", async () => {
    let reads = 0;
    const fetchFn = (async () => {
      if (reads++ === 0) throw new TypeError("Failed to fetch");
      const record = { total: "1250000000000000000000", available: "0" };
      return new Response(JSON.stringify({ result: { result: [...new TextEncoder().encode(JSON.stringify(record))] } }));
    }) as typeof fetch;
    const chain = chainReader({ rpcUrl: "http://127.0.0.1:1", coin: COIN }, fetchFn, { sleep: async () => {} });
    let sends = 0;
    const session = fakeSession(async () => {
      sends++;
      throw new TypeError("Failed to fetch");
    });
    const out = await sendBuy({ chain, session, coin: COIN }, previewed());
    expect(reads).toBe(2);
    // A send that fails is never sent again: the wallet is asked once.
    expect(sends).toBe(1);
    expect(out.kind).toBe("unknown");
  });
});

const SELLER = "cabal2-later-buyer.fairchance.testnet";
const COIN2 = "cabal2.factory.fairchance.testnet";
const fullQuote = { tokens_filled: "2709426042412763046439220", near_out: "292223155027538688590017", penalty_near: "0", fill: "full", limit_reason: null };

describe("a sell", () => {
  it("sends the builder's min_near_out (the quoted NEAR minus 1%) and attaches 1 yoctoNEAR", async () => {
    const chain = fakeChain({ fc_quote_sell: fullQuote });
    const session = fakeSession(async () => [fixture("cabal2-sell-full")], SELLER);
    const out = await sendSell({ chain, session, coin: COIN2 }, 2709426042412763046439220n);
    expect(chain.calls[0]).toEqual(["fc_quote_sell", { account_id: SELLER, amount: "2709426042412763046439220" }]);
    const action = session.sent[0]![0]!.actions[0]!;
    expect(action.params.methodName).toBe("sell");
    expect(action.params.args).toEqual({ amount: "2709426042412763046439220", min_near_out: "289300923477263301704116" });
    expect(action.params.deposit).toBe("1");
    expect(out).toMatchObject({ kind: "result", result: { kind: "sold", fill: "full" } });
  });

  it("sends the builder's floor for a quote that fills in part: the filled price, scaled to all of amount, minus 1%", async () => {
    const partial = { tokens_filled: "1000", near_out: "500", penalty_near: "0", fill: "partial", limit_reason: "allowance" };
    const chain = fakeChain({ fc_quote_sell: partial });
    const session = fakeSession(async () => [fixture("cabal2-sell-partial")], SELLER);
    await sendSell({ chain, session, coin: COIN2 }, 5000n);
    expect(session.sent[0]![0]!.actions[0]!.params.args).toEqual({ amount: "5000", min_near_out: "2475" });
  });

  it("sends nothing when no part can sell now, and says why", async () => {
    const none = { tokens_filled: "0", near_out: "0", penalty_near: "0", fill: "none", limit_reason: "cap_full" };
    const chain = fakeChain({ fc_quote_sell: none });
    const session = fakeSession(async () => [], SELLER);
    expect(await sendSell({ chain, session, coin: COIN2 }, 5000n)).toEqual({ kind: "not-sent", text: "None of it can sell now, so nothing was sent. The pool cap is full for this hour." });
    expect(session.sent).toHaveLength(0);
  });
});

describe("a sell of more than the account holds", () => {
  it("sends nothing and says so when the coin gives no quote (the amount is above the balance)", async () => {
    const chain = fakeChain({ fc_quote_sell: null, fc_account: { class: "small", balance: "1000", is_large: false, can_send: true } });
    const session = fakeSession(async () => [], SELLER);
    expect(await sendSell({ chain, session, coin: COIN2 }, 5000n)).toEqual({ kind: "not-sent", text: "You hold less than this amount now. Nothing was sent." });
    expect(session.sent).toHaveLength(0);
  });
});

describe("a sell the coin refuses in the early window", () => {
  const early = { class: "small", balance: "5000", marked_balance: "5000", unmarked_balance: "0", is_large: false, can_send: true };
  const window = { early_blocks: 17, blocks_left: 6, early_supply_total: "5000", shared_allowance: "100", buy_fee_bps_now: 0, fee_end_block: 17 };
  it("says early supply cannot sell yet, not that the visitor holds less", async () => {
    const chain = fakeChain({ fc_quote_sell: null, fc_account: early, fc_early_window: window });
    const session = fakeSession(async () => [], SELLER);
    expect(await sendSell({ chain, session, coin: COIN2 }, 1n)).toEqual({ kind: "not-sent", text: "Early supply cannot sell until block 17 has passed, in about 6 blocks. Nothing was sent." });
  });
  it("gives a sell limit of 0 now with the early window reason", async () => {
    const chain = fakeChain({ fc_quote_sell: null, fc_account: early, fc_early_window: window });
    expect(await loadSellLimit(chain, SELLER, { symbol: "BAG", decimals: 24 })).toEqual({ balance: 5000n, canSellNow: 0n, moreOpens: "Early supply cannot sell until block 17 has passed, in about 6 blocks." });
  });
});

describe("what the account can sell now", () => {
  const account = { class: "small", balance: "2709426042412763046439220", is_large: false, can_send: true };

  it("is the part of the whole balance the coin quotes as filled now", async () => {
    const chain = fakeChain({ fc_account: account, fc_quote_sell: { ...fullQuote, tokens_filled: "1000000000000000000000000", fill: "partial", limit_reason: "allowance" } });
    expect(await loadSellLimit(chain, SELLER, { symbol: "BAG", decimals: 24 })).toEqual({ balance: 2709426042412763046439220n, canSellNow: 1000000000000000000000000n, moreOpens: null });
    expect(chain.calls[1]).toEqual(["fc_quote_sell", { account_id: SELLER, amount: "2709426042412763046439220" }]);
  });

  it("reads again once when the balance changed between its two reads", async () => {
    let n = 0;
    const chain = fakeChain({ fc_account: () => (n++ === 0 ? account : { ...account, balance: "0" }), fc_quote_sell: null });
    expect(await loadSellLimit(chain, SELLER, { symbol: "BAG", decimals: 24 })).toEqual({ balance: 0n, canSellNow: 0n, moreOpens: null });
  });

  it("is zero without a quote when the account holds none", async () => {
    const chain = fakeChain({ fc_account: { ...account, balance: "0" } });
    expect(await loadSellLimit(chain, SELLER, { symbol: "BAG", decimals: 24 })).toEqual({ balance: 0n, canSellNow: 0n, moreOpens: null });
    expect(chain.calls).toHaveLength(1);
  });
});

describe("a launch", () => {
  const form = { name: "Cabal Two", ticker: "cabal2", description: "", openingBuy: "" };

  it("calls the factory with the 0.25 NEAR deposit plus the opening buy, and reads the launch", async () => {
    const session = fakeSession(async () => [fixture("cabal2-launch")], "cabal2-creator.fairchance.testnet");
    const out = await sendLaunch(session, "factory.fairchance.testnet", { ...form, openingBuy: "0.5" }, null);
    const [tx] = session.sent[0]!;
    expect(tx!.receiverId).toBe("factory.fairchance.testnet");
    expect(tx!.actions[0]!.params.methodName).toBe("launch");
    expect(tx!.actions[0]!.params.deposit).toBe("750000000000000000000000");
    expect(tx!.actions[0]!.params.args).toMatchObject({ name: "Cabal Two", ticker: "CABAL2", opening_buy: "500000000000000000000000" });
    expect(out).toMatchObject({ kind: "result", result: { kind: "launched", coin: "cabal2.factory.fairchance.testnet", creator: "cabal2-creator.fairchance.testnet" } });
  });

  it("sends nothing for a form the factory would refuse", async () => {
    const session = fakeSession(async () => []);
    expect(await sendLaunch(session, "factory.fairchance.testnet", { ...form, ticker: "" }, null)).toEqual({ kind: "not-sent", text: "Type a ticker." });
    expect(session.sent).toHaveLength(0);
  });
});
