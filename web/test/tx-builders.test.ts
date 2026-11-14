// The transactions the page asks the wallet to sign. The expected actions match what the recorded
// testnet runs sent (method, args shape, 1 yoctoNEAR on a sell), with the minimums from the quote.
import { describe, expect, it } from "vitest";
import { buildBuy, buildClaimCreatorFees, buildLaunch, buildSell, readBuyerStorage } from "../src/tx-builders.ts";
import type { SellQuote } from "../src/account-views.ts";
import type { BuyQuote } from "../src/buy-quote.ts";

const COIN = "cabal2.factory.fairchance.testnet";
const quote = (tokensOut: bigint): BuyQuote => ({
  tokensOut,
  feeNear: 0n,
  creatorFeeNear: 0n,
  markedTokens: 0n,
  classAfter: "small",
  becomesLarge: false,
  allowanceAfter: 0n,
});

describe("a buy", () => {
  it("is one buy action with the NEAR attached and min_tokens_out from the quote, for a registered buyer", () => {
    expect(buildBuy(COIN, { nearIn: 300000000000000000000000n, quote: quote(2709426042412763046439220n), storage: { registered: true } })).toEqual([
      {
        receiverId: COIN,
        actions: [
          {
            type: "FunctionCall",
            params: { methodName: "buy", args: { min_tokens_out: "2709426042412763046439220" }, gas: "50000000000000", deposit: "300000000000000000000000" },
          },
        ],
      },
    ]);
  });
});

describe("a first buy", () => {
  it("puts storage_deposit with the coin's storage minimum before the buy, in the same transaction", () => {
    const txs = buildBuy(COIN, { nearIn: 10n ** 24n, quote: quote(7n), storage: { registered: false, storageMin: 3200000000000000000000n } });
    expect(txs).toHaveLength(1);
    expect(txs[0]!.actions.map((a) => [a.params.methodName, a.params.args, a.params.deposit])).toEqual([
      ["storage_deposit", {}, "3200000000000000000000"],
      ["buy", { min_tokens_out: "7" }, "1000000000000000000000000"],
    ]);
  });
});

describe("reading the buyer's storage record", () => {
  const chain = (views: Record<string, unknown>) => ({
    view: async <T,>(method: string, args?: object): Promise<T> => {
      if (method === "storage_balance_of") expect(args).toEqual({ account_id: "alice.testnet" });
      return views[method] as T;
    },
  });

  it("is registered when storage_balance_of returns a record", async () => {
    expect(await readBuyerStorage(chain({ storage_balance_of: { total: "0", available: "0" } }), "alice.testnet")).toEqual({ registered: true });
  });

  it("is not registered when storage_balance_of returns null, with the minimum from storage_balance_bounds", async () => {
    expect(
      await readBuyerStorage(chain({ storage_balance_of: null, storage_balance_bounds: { min: "3200000000000000000000", max: "3200000000000000000000" } }), "alice.testnet"),
    ).toEqual({ registered: false, storageMin: 3200000000000000000000n });
  });
});

const sellQuote = (tokensFilled: bigint, nearOut: bigint, fill: SellQuote["fill"]): SellQuote => ({ tokensFilled, nearOut, penaltyNear: 0n, fill, limitReason: fill === "full" ? null : "floor" });

describe("a sell", () => {
  it("is one sell action with exactly 1 yoctoNEAR and min_near_out from the quote's near_out", () => {
    expect(buildSell(COIN, { amount: 2709426042412763046439220n, quote: sellQuote(2709426042412763046439220n, 292223155027538688590017n, "full") })).toEqual([
      {
        receiverId: COIN,
        actions: [
          {
            type: "FunctionCall",
            params: {
              methodName: "sell",
              args: { amount: "2709426042412763046439220", min_near_out: "292223155027538688590017" },
              gas: "100000000000000",
              deposit: "1",
            },
          },
        ],
      },
    ]);
  });

  it("asks for the whole amount on a quoted partial fill; the coin fills what the limits allow", () => {
    const [tx] = buildSell(COIN, { amount: 3970605075343742132738487n, quote: sellQuote(96425179448780606817852n, 10440403632388116519974n, "partial") });
    expect(tx!.actions[0]!.params.args).toEqual({ amount: "3970605075343742132738487", min_near_out: "10440403632388116519974" });
  });
});

describe("a creator fee claim", () => {
  it("is one claim_creator_fees action with exactly 1 yoctoNEAR", () => {
    expect(buildClaimCreatorFees(COIN)).toEqual([
      { receiverId: COIN, actions: [{ type: "FunctionCall", params: { methodName: "claim_creator_fees", args: {}, gas: "300000000000000", deposit: "1" } }] },
    ]);
  });
});

describe("a launch", () => {
  const FACTORY = "factory.fairchance.testnet";
  const form = { name: "Cabal Test 2", ticker: "cabal2", description: "", openingBuy: "" };
  const ICON_PREFIX = "data:image/webp;base64,";

  it("calls the factory's launch with 1 NEAR for the coin's storage, as the testnet launch did, and names the new coin", () => {
    expect(buildLaunch(FACTORY, { form, icon: null })).toEqual({
      kind: "ready",
      coin: "cabal2.factory.fairchance.testnet",
      deposit: 10n ** 24n,
      txs: [
        {
          receiverId: FACTORY,
          actions: [
            {
              type: "FunctionCall",
              params: {
                methodName: "launch",
                args: { name: "Cabal Test 2", ticker: "CABAL2", icon: null, description: null, opening_buy: null },
                gas: "300000000000000",
                deposit: "1000000000000000000000000",
              },
            },
          ],
        },
      ],
    });
  });

  it("adds the opening buy to the deposit and passes it in yoctoNEAR", () => {
    const r = buildLaunch(FACTORY, { form: { ...form, description: " A coin. ", openingBuy: "0.5" }, icon: `${ICON_PREFIX}UklGRg==` });
    if (r.kind !== "ready") throw new Error(r.errors.join(" "));
    expect(r.deposit).toBe(1_500_000_000_000_000_000_000_000n);
    expect(r.txs[0]!.actions[0]!.params).toMatchObject({
      deposit: "1500000000000000000000000",
      args: { description: "A coin.", icon: `${ICON_PREFIX}UklGRg==`, opening_buy: "500000000000000000000000" },
    });
  });

  it("returns the form's errors and builds nothing for a form that breaks a rule", () => {
    expect(buildLaunch(FACTORY, { form: { ...form, ticker: "a.b" }, icon: null })).toEqual({ kind: "invalid", errors: ["The ticker can use letters, digits, - and _ only."] });
  });

  it("accepts an icon of 8191 bytes, the coin's limit, and refuses one of 8192 with the shared sentence", () => {
    const icon = (bytes: number) => ICON_PREFIX + "A".repeat(bytes - ICON_PREFIX.length);
    expect(buildLaunch(FACTORY, { form, icon: icon(8191) }).kind).toBe("ready");
    expect(buildLaunch(FACTORY, { form, icon: icon(8192) })).toEqual({ kind: "invalid", errors: ["The coin icon must be a WebP image under 8 KB."] });
  });

  it("refuses an icon that is not a WebP data URL", () => {
    expect(buildLaunch(FACTORY, { form, icon: "data:image/png;base64,iVBORw0KGgo=" }).kind).toBe("invalid");
    expect(buildLaunch(FACTORY, { form, icon: ICON_PREFIX }).kind).toBe("invalid");
    expect(buildLaunch(FACTORY, { form, icon: `${ICON_PREFIX}not base64!` }).kind).toBe("invalid");
  });
});

describe("values the caller can set in place of the defaults", () => {
  it("uses a given min_tokens_out in place of the quote's tokens_out", () => {
    const [tx] = buildBuy(COIN, { nearIn: 1n, quote: quote(1000n), storage: { registered: true }, minTokensOut: 990n });
    expect(tx!.actions[0]!.params.args).toEqual({ min_tokens_out: "990" });
  });

  it("uses a given min_near_out in place of the quote's near_out", () => {
    const [tx] = buildSell(COIN, { amount: 10n, quote: sellQuote(10n, 500n, "full"), minNearOut: 0n });
    expect(tx!.actions[0]!.params.args).toEqual({ amount: "10", min_near_out: "0" });
  });

  it("uses a given storage deposit for a launch in place of 1 NEAR, still adding the opening buy", () => {
    const r = buildLaunch("factory.fairchance.testnet", {
      form: { name: "Cabal Test 2", ticker: "cabal2", description: "", openingBuy: "0.5" },
      icon: null,
      storageDeposit: 250_000_000_000_000_000_000_000n,
    });
    expect(r).toMatchObject({ kind: "ready", deposit: 750_000_000_000_000_000_000_000n });
  });
});
