// The outcome parser on real testnet results: each fixture is the EXPERIMENTAL_tx_status result of
// one transaction from the recorded runs (evidence/testnet-cabal2-run.json, evidence/testnet-run.json).
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { FinalOutcome } from "../src/tx-types.ts";
import { explorerTxLink, parseTxOutcome } from "../src/tx-outcome.ts";

const fixture = (name: string) =>
  JSON.parse(readFileSync(join(import.meta.dirname, "fixtures/tx-outcomes", `${name}.json`), "utf8")) as FinalOutcome;

describe("a sell result", () => {
  it("reads a full fill: the later buyer sold all 2.7M tokens for 0.2922 NEAR", () => {
    expect(parseTxOutcome(fixture("cabal2-sell-full"))).toEqual({
      kind: "sold",
      txHash: "A6j6eFJ664trF575bTzaecPuzv4kUTtkmy5ZiGrcMtmh",
      explorerUrl: "https://testnet.nearblocks.io/txns/A6j6eFJ664trF575bTzaecPuzv4kUTtkmy5ZiGrcMtmh",
      account: "cabal2-later-buyer.fairchance.testnet",
      fill: "full",
      tokensAsked: 2709426042412763046439220n,
      tokensSold: 2709426042412763046439220n,
      tokensKept: 0n,
      nearOut: 292223155027538688590017n,
      penaltyNear: 0n,
      limitReason: null,
    });
  });
});

describe("a partial sell", () => {
  it("reads the floor limit: buyer 01 asked 3.97M, sold 96.4K for 0.0104 NEAR and kept the rest", () => {
    const r = parseTxOutcome(fixture("cabal2-sell-partial"));
    expect(r).toMatchObject({
      kind: "sold",
      txHash: "4RPq2EpBoBynMssXJwP7Pf5QgReEuQdvPYf8Kdqws7Mu",
      fill: "partial",
      tokensAsked: 3970605075343742132738487n,
      tokensSold: 96425179448780606817852n,
      tokensKept: 3970605075343742132738487n - 96425179448780606817852n,
      nearOut: 10440403632388116519974n,
      limitReason: "floor",
    });
  });

  it("reads the cap limit on the BAG coin: buyer 17 sold 8.74M of 9.43M for 0.9205 NEAR", () => {
    expect(parseTxOutcome(fixture("bag-sell-partial"))).toMatchObject({
      kind: "sold",
      account: "bag-buyer-17.fairchance.testnet",
      fill: "partial",
      tokensSold: 8737868038700155563510651n,
      tokensKept: 9426505449507747827819514n - 8737868038700155563510651n,
      nearOut: 920495437893996097270282n,
      limitReason: "cap_full",
    });
  });
});

describe("a refused transaction", () => {
  it("reads FC_EARLY_WINDOW from the failed receipt and gives the shared refusal sentence", () => {
    expect(parseTxOutcome(fixture("cabal2-sell-early-window"))).toEqual({
      kind: "refused",
      txHash: "HHY8oh2HmB9TNqf5uurRLEmG9Lu5JFpAnqT8AWVL8j3t",
      explorerUrl: "https://testnet.nearblocks.io/txns/HHY8oh2HmB9TNqf5uurRLEmG9Lu5JFpAnqT8AWVL8j3t",
      code: "FC_EARLY_WINDOW",
      text: "Early supply cannot sell in the early blocks, and selling opens when the early window ends.",
      chainMessage: "Smart contract panicked: FC_EARLY_WINDOW",
    });
  });
});

/** A real outcome with its first receipt to `executor` turned into a contract panic with `message`. */
function withPanic(name: string, executor: string, message: string): FinalOutcome {
  const o = fixture(name);
  const receipt = o.receipts_outcome.find((r) => r.outcome.executor_id === executor)!;
  const failure = { ActionError: { index: 0, kind: { FunctionCallError: { ExecutionError: message } } } };
  receipt.outcome.status = { Failure: failure };
  receipt.outcome.logs = [];
  o.status = { Failure: failure };
  return o;
}

describe("a buy result", () => {
  it("reads a buy sent with its storage deposit: the later buyer paid 0.3 NEAR for 2.71M tokens, none marked", () => {
    expect(parseTxOutcome(fixture("cabal2-buy-later"))).toEqual({
      kind: "bought",
      txHash: "9k4WFfawxaGxzHEa91JGouyByesSymi3u9ViDANVXzZa",
      explorerUrl: "https://testnet.nearblocks.io/txns/9k4WFfawxaGxzHEa91JGouyByesSymi3u9ViDANVXzZa",
      account: "cabal2-later-buyer.fairchance.testnet",
      nearIn: 300000000000000000000000n,
      tokensOut: 2709426042412763046439220n,
      markedTokens: 0n,
      feeNear: 3000000000000000000000n,
    });
  });

  it("reads an early buy whose tokens are all marked", () => {
    expect(parseTxOutcome(fixture("cabal2-buy-early"))).toMatchObject({
      kind: "bought",
      tokensOut: 3970605075343742132738487n,
      markedTokens: 3970605075343742132738487n,
    });
  });

  it("reads a BAG coin buy of 1 NEAR", () => {
    expect(parseTxOutcome(fixture("bag-buy"))).toMatchObject({ kind: "bought", nearIn: 10n ** 24n, tokensOut: 7308624762134838070425063n });
  });

  it("gives the storage sentence for a buy from an account with no storage record", () => {
    const o = withPanic("bag-buy", "bag.factory.fairchance.testnet", "Smart contract panicked: The account is not registered: bag-buyer-01.fairchance.testnet");
    expect(parseTxOutcome(o)).toMatchObject({
      kind: "refused",
      code: null,
      text: "That account is not registered on this coin, so it must pay the storage deposit before it can buy or receive tokens.",
    });
  });

  it("gives the slippage sentence for FC_SLIPPAGE", () => {
    const o = withPanic("cabal2-buy-later", "cabal2.factory.fairchance.testnet", "Smart contract panicked: FC_SLIPPAGE");
    expect(parseTxOutcome(o)).toMatchObject({ kind: "refused", code: "FC_SLIPPAGE", text: "The price moved below your minimum, so try again or lower your minimum." });
  });
});

describe("a launch result", () => {
  it("reads the new coin from the factory's fc_launch event", () => {
    expect(parseTxOutcome(fixture("cabal2-launch"))).toEqual({
      kind: "launched",
      txHash: "BgzcrdkorKb9iLzoS4a4WqVrTkL8ihXssb15wAmpw5RP",
      explorerUrl: "https://testnet.nearblocks.io/txns/BgzcrdkorKb9iLzoS4a4WqVrTkL8ihXssb15wAmpw5RP",
      coin: "cabal2.factory.fairchance.testnet",
      creator: "cabal2-creator.fairchance.testnet",
      launchBlock: 277642845,
      description: "Testnet coin for the Fair Chance sniper test. Ten wallets funded by one wallet buy in the first blocks and try to sell at once.",
      openingTokens: 0n,
    });
  });

  it("reads a launch with an opening buy as a launch, with the creator's opening tokens", () => {
    const o = fixture("cabal2-launch");
    const coinStart = o.receipts_outcome.find((r) => r.outcome.executor_id === "cabal2.factory.fairchance.testnet")!;
    coinStart.outcome.logs.push(
      'EVENT_JSON:{"standard":"fair_chance","version":"1.0.0-rc.7","event":"fc_buy","data":[{"account":"cabal2-creator.fairchance.testnet","near_in":"100000000000000000000000","fee_near":"0","creator_fee_near":"0","tokens_out":"990000000000000000000000","marked_tokens":"990000000000000000000000","class_after":"small","block":277642845}]}',
    );
    expect(parseTxOutcome(o)).toMatchObject({ kind: "launched", coin: "cabal2.factory.fairchance.testnet", openingTokens: 990000000000000000000000n });
  });

  it("reports a launch whose coin start failed, with the chain's text", () => {
    const o = withPanic("cabal2-launch", "cabal2.factory.fairchance.testnet", "Smart contract panicked: the launch deposit does not cover the coin's storage");
    expect(parseTxOutcome(o)).toMatchObject({ kind: "failed", chainMessage: "Smart contract panicked: the launch deposit does not cover the coin's storage" });
  });
});

describe("any other failure", () => {
  it("keeps the chain's text when the refusal file has no sentence for it", () => {
    const o = withPanic("cabal2-sell-full", "cabal2.factory.fairchance.testnet", "Exceeded the prepaid gas.");
    expect(parseTxOutcome(o)).toEqual({
      kind: "failed",
      txHash: "A6j6eFJ664trF575bTzaecPuzv4kUTtkmy5ZiGrcMtmh",
      explorerUrl: "https://testnet.nearblocks.io/txns/A6j6eFJ664trF575bTzaecPuzv4kUTtkmy5ZiGrcMtmh",
      chainMessage: "Exceeded the prepaid gas.",
    });
  });

  it("gives the JSON of a failure that is not a contract panic", () => {
    const o = fixture("cabal2-sell-full");
    o.receipts_outcome[0]!.outcome.status = { Failure: { ActionError: { index: 0, kind: { AccountDoesNotExist: { account_id: "nobody.testnet" } } } } };
    expect(parseTxOutcome(o)).toMatchObject({ kind: "failed", chainMessage: '{"ActionError":{"index":0,"kind":{"AccountDoesNotExist":{"account_id":"nobody.testnet"}}}}' });
  });

  it("reports a successful transaction with no Fair Chance event as failed, not as a trade", () => {
    const o = fixture("cabal2-sell-full");
    for (const r of o.receipts_outcome) r.outcome.logs = [];
    expect(parseTxOutcome(o)).toMatchObject({ kind: "failed", chainMessage: "The transaction succeeded but logged no Fair Chance event." });
  });
});

describe("a creator fee claim result", () => {
  // No claim ran on testnet yet; this outcome has the fc_creator_claim event shape of the interface.
  it("reads the amount paid from fc_creator_claim", () => {
    const o = fixture("cabal2-sell-full");
    o.receipts_outcome[0]!.outcome.logs = [
      'EVENT_JSON:{"standard":"fair_chance","version":"1.0.0-rc.7","event":"fc_creator_claim","data":[{"creator":"cabal2-creator.fairchance.testnet","amount":"420000000000000000000000","depth_ok_since_ns":"1794651086885757960","block":277700000}]}',
    ];
    expect(parseTxOutcome(o)).toMatchObject({ kind: "claimed", creator: "cabal2-creator.fairchance.testnet", amount: 420000000000000000000000n });
  });

  it("reports a claim whose transfer failed and whose callback logged no event", () => {
    const o = fixture("cabal2-sell-full");
    for (const r of o.receipts_outcome) r.outcome.logs = [];
    o.receipts_outcome[1]!.outcome.status = { Failure: { ActionError: { index: 0, kind: { AccountDoesNotExist: { account_id: "gone.testnet" } } } } };
    expect(parseTxOutcome(o).kind).toBe("failed");
  });
});

describe("the explorer link", () => {
  it("is the testnet explorer page of the hash", () => {
    expect(explorerTxLink("BgzcrdkorKb9iLzoS4a4WqVrTkL8ihXssb15wAmpw5RP")).toBe("https://testnet.nearblocks.io/txns/BgzcrdkorKb9iLzoS4a4WqVrTkL8ihXssb15wAmpw5RP");
  });
});

describe("text from the chain", () => {
  it("drops direction and control characters from event text and failure text, as the page does for views", () => {
    const o = fixture("cabal2-launch");
    const factory = o.receipts_outcome.find((r) => r.outcome.logs.some((l) => l.includes("fc_launch")))!;
    factory.outcome.logs = factory.outcome.logs.map((l) => l.replace('"description":"Testnet coin', '"description":"\\u202eTestnet\\u0007 coin'));
    expect(parseTxOutcome(o)).toMatchObject({ kind: "launched", description: expect.stringMatching(/^Testnet coin for/) });
    const failed = withPanic("cabal2-sell-full", "cabal2.factory.fairchance.testnet", "Smart contract panicked: ‮boom");
    expect(parseTxOutcome(failed)).toMatchObject({ kind: "failed", chainMessage: "Smart contract panicked: boom" });
  });
});

/** A receipt of another contract, added to a real outcome. */
const foreignReceipt = (executor: string, log: string) => ({ id: "x", block_hash: "y", outcome: { status: { SuccessValue: "" }, logs: [log], executor_id: executor } });
const FAKE_SELL =
  'EVENT_JSON:{"standard":"fair_chance","version":"1.0.0","event":"fc_sell","data":[{"account":"victim.testnet","class":"small","tokens_asked":"5","tokens_sold":"5","near_out":"999","penalty_near":"0","fill":"full","limit_reason":null}]}';

describe("events from the right contract only", () => {
  it("ignores a Fair Chance event logged by a contract the transaction did not target", () => {
    const o = fixture("cabal2-buy-later");
    o.receipts_outcome.unshift(foreignReceipt("evil.testnet", FAKE_SELL));
    expect(parseTxOutcome(o)).toMatchObject({ kind: "bought", tokensOut: 2709426042412763046439220n });
  });

  it("ignores an fc_launch logged by any account but the factory the transaction called", () => {
    const o = fixture("cabal2-buy-later");
    o.receipts_outcome.unshift(foreignReceipt("evil.testnet", 'EVENT_JSON:{"standard":"fair_chance_factory","version":"1","event":"fc_launch","data":[{"coin":"x.testnet","creator":"y.testnet","launch_block":1,"description":null}]}'));
    expect(parseTxOutcome(o).kind).toBe("bought");
  });

  it("reads the opening fc_buy of a launch only from the new coin, a sub-account of the factory", () => {
    const o = fixture("cabal2-launch");
    o.receipts_outcome.push(
      foreignReceipt(
        "evil.testnet",
        'EVENT_JSON:{"standard":"fair_chance","version":"1","event":"fc_buy","data":[{"account":"a","near_in":"1","fee_near":"0","creator_fee_near":"0","tokens_out":"5","marked_tokens":"5","class_after":"small","block":1}]}',
      ),
    );
    expect(parseTxOutcome(o)).toMatchObject({ kind: "launched", openingTokens: 0n });
  });

  it("refuses an fc_launch whose coin is not a sub-account of the factory", () => {
    const o = fixture("cabal2-launch");
    const factory = o.receipts_outcome.find((r) => r.outcome.logs.some((l) => l.includes("fc_launch")))!;
    factory.outcome.logs = factory.outcome.logs.map((l) => l.replace('"coin":"cabal2.factory.fairchance.testnet"', '"coin":"cabal2.evil.testnet"'));
    expect(parseTxOutcome(o)).toMatchObject({ kind: "failed", chainMessage: "The transaction result has a Fair Chance event this page cannot read." });
  });
});

describe("a result that cannot be read", () => {
  it("does not throw on a malformed event from another contract", () => {
    const o = fixture("cabal2-buy-later");
    o.receipts_outcome.push(foreignReceipt("other.testnet", 'EVENT_JSON:{"standard":"x","event":"y","data":{}}'));
    expect(parseTxOutcome(o).kind).toBe("bought");
  });

  it.each([
    ["data that is not a list", 'EVENT_JSON:{"standard":"fair_chance","event":"fc_sell","data":{}}'],
    ["JSON that does not parse", "EVENT_JSON:{not json"],
    ["an amount that is not a number", FAKE_SELL.replace('"near_out":"999"', '"near_out":"lots"')],
    ["a fill that is not full, partial or none", FAKE_SELL.replace('"fill":"full"', '"fill":"most"')],
    ["a limit reason the interface does not name", FAKE_SELL.replace('"limit_reason":null', '"limit_reason":"vibes"')],
  ])("gives failed with a plain sentence for %s from the coin itself", (_name, log) => {
    const o = fixture("cabal2-sell-full");
    for (const r of o.receipts_outcome) r.outcome.logs = [];
    o.receipts_outcome[0]!.outcome.logs = [log];
    expect(parseTxOutcome(o)).toMatchObject({ kind: "failed", chainMessage: "The transaction result has a Fair Chance event this page cannot read." });
  });

  it("gives failed for an outcome with no receipts list", () => {
    const o = fixture("cabal2-sell-full") as unknown as Record<string, unknown>;
    delete o.receipts_outcome;
    expect(parseTxOutcome(o as unknown as FinalOutcome).kind).toBe("failed");
  });
});

describe("the explorer link of an odd hash", () => {
  it.each(["abc/../../x?y#z", 'x/../../evil?a=1#"><b>', "", "0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OIl0OI", "A6j6eFJ664trF575bTzaecPuzv4kUTtkmy5ZiGrcMtmhX"])("gives no link for %j", (hash) => {
    expect(explorerTxLink(hash)).toBeNull();
  });

  it("gives no link in the result when the transaction hash is not base58 of 43 or 44 characters", () => {
    const o = fixture("cabal2-sell-full");
    o.transaction.hash = "abc/../../x?y#z";
    expect(parseTxOutcome(o)).toMatchObject({ kind: "sold", explorerUrl: null });
  });

  it("links a 43-character hash", () => {
    expect(explorerTxLink("K5YbpnT1N9or2hbKk9ZxbjuVj6khCaoSnPebcQpZhHG")).toBe("https://testnet.nearblocks.io/txns/K5YbpnT1N9or2hbKk9ZxbjuVj6khCaoSnPebcQpZhHG");
  });
});
