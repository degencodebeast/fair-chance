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
  it("is one buy action with the NEAR attached and min_tokens_out 1% below the quote, for a registered buyer", () => {
    expect(buildBuy(COIN, { nearIn: 300000000000000000000000n, quote: quote(2709426042412763046439220n), storage: { registered: true } })).toEqual([
      {
        receiverId: COIN,
        actions: [
          {
            type: "FunctionCall",
            params: { methodName: "buy", args: { min_tokens_out: "2682331781988635415974827" }, gas: "50000000000000", deposit: "300000000000000000000000" },
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
      ["buy", { min_tokens_out: "6" }, "1000000000000000000000000"],
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

/** The coin's slippage check (rule-engine actions.rs, product_at_least): a sell is refused when near_out * amount < min_near_out * tokens_sold. */
const coinAcceptsSell = (nearOut: bigint, amount: bigint, minNearOut: bigint, tokensSold: bigint) => nearOut * amount >= minNearOut * tokensSold;
const minNearOutOf = (r: ReturnType<typeof buildSell>) => {
  if (r.kind !== "ready") throw new Error(r.text);
  return BigInt(r.txs[0]!.actions[0]!.params.args.min_near_out as string);
};

describe("a sell", () => {
  it("is one sell action with exactly 1 yoctoNEAR and min_near_out 1% below the quote's near_out", () => {
    expect(buildSell(COIN, { amount: 2709426042412763046439220n, quote: sellQuote(2709426042412763046439220n, 292223155027538688590017n, "full") })).toEqual({
      kind: "ready",
      txs: [
        {
          receiverId: COIN,
          actions: [
            {
              type: "FunctionCall",
              params: {
                methodName: "sell",
                args: { amount: "2709426042412763046439220", min_near_out: "289300923477263301704116" },
                gas: "100000000000000",
                deposit: "1",
              },
            },
          ],
        },
      ],
    });
  });

  describe("on a quoted partial fill (the real CABAL2 buyer 01 sell: asked 3.97M, quote fills 96.4K for 0.0104 NEAR)", () => {
    const amount = 3970605075343742132738487n;
    const tokensFilled = 96425179448780606817852n;
    const nearOut = 10440403632388116519974n;
    const r = buildSell(COIN, { amount, quote: sellQuote(tokensFilled, nearOut, "partial") });

    it("asks for the whole amount and scales min_near_out to all of it at the quoted price per token, less 1%", () => {
      expect(r.kind === "ready" && r.txs[0]!.actions[0]!.params.args).toEqual({ amount: "3970605075343742132738487", min_near_out: "425616759953071735207344" });
    });

    it("passes under the coin's check when the same part fills at a price 0.5% lower", () => {
      expect(coinAcceptsSell((nearOut * 995n) / 1000n, amount, minNearOutOf(r), tokensFilled)).toBe(true);
    });

    it("is refused with FC_SLIPPAGE under the coin's check when the price drops 2%, and when it drops 97%", () => {
      expect(coinAcceptsSell((nearOut * 98n) / 100n, amount, minNearOutOf(r), tokensFilled)).toBe(false);
      expect(coinAcceptsSell((nearOut * 3n) / 100n, amount, minNearOutOf(r), tokensFilled)).toBe(false);
    });

    it("holds the same price floor when a different part fills than quoted", () => {
      const half = tokensFilled / 2n;
      expect(coinAcceptsSell((nearOut * 995n) / 2000n, amount, minNearOutOf(r), half)).toBe(true);
      expect(coinAcceptsSell((nearOut * 98n) / 200n, amount, minNearOutOf(r), half)).toBe(false);
    });
  });

  it("builds nothing when the quote fills no tokens, and says so", () => {
    expect(buildSell(COIN, { amount: 5n, quote: { tokensFilled: 0n, nearOut: 0n, penaltyNear: 0n, fill: "none", limitReason: "cap_full" } })).toEqual({
      kind: "nothing-fills",
      text: "No tokens can sell now, so there is no sell to send.",
    });
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

  it("calls the factory's launch with 0.25 NEAR for the coin's storage and names the new coin", () => {
    expect(buildLaunch(FACTORY, { form, icon: null })).toEqual({
      kind: "ready",
      coin: "cabal2.factory.fairchance.testnet",
      deposit: 250_000_000_000_000_000_000_000n,
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
                deposit: "250000000000000000000000",
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
    expect(r.deposit).toBe(750_000_000_000_000_000_000_000n);
    expect(r.txs[0]!.actions[0]!.params).toMatchObject({
      deposit: "750000000000000000000000",
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

  it("builds nothing for a ticker with a letter outside ASCII, such as the Kelvin sign", () => {
    expect(buildLaunch(FACTORY, { form: { ...form, ticker: "\u212A" }, icon: null })).toEqual({ kind: "invalid", errors: ["The ticker can use letters, digits, - and _ only."] });
  });

  it("refuses an icon that is not a WebP data URL", () => {
    expect(buildLaunch(FACTORY, { form, icon: "data:image/png;base64,iVBORw0KGgo=" }).kind).toBe("invalid");
    expect(buildLaunch(FACTORY, { form, icon: ICON_PREFIX }).kind).toBe("invalid");
    expect(buildLaunch(FACTORY, { form, icon: `${ICON_PREFIX}not base64!` }).kind).toBe("invalid");
  });
});

describe("a launch storage deposit the caller sets", () => {
  it("uses a given storage deposit for a launch in place of 0.25 NEAR, still adding the opening buy", () => {
    const r = buildLaunch("factory.fairchance.testnet", {
      form: { name: "Cabal Test 2", ticker: "cabal2", description: "", openingBuy: "0.5" },
      icon: null,
      storageDeposit: 200_000_000_000_000_000_000_000n,
    });
    expect(r).toMatchObject({ kind: "ready", deposit: 700_000_000_000_000_000_000_000n });
  });
});
