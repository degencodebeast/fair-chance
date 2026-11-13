import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import type { ChainReader } from "../src/chain.ts";
import { ViewNotAvailable } from "../src/chain.ts";
import { NOT_AVAILABLE } from "../src/coin-page-model.ts";
import { parseTokenAmount } from "../src/token-amount.ts";
import { loadAccountPlan } from "../src/your-plan.ts";
import { yourPlanView } from "../src/your-plan-model.ts";
import { allText, copyViolations } from "./copy-rules.ts";
import { STATES } from "./recorded-coin.ts";

interface AccountState {
  description: string;
  now_ns: string;
  symbol: string;
  decimals: number;
  total_supply: string;
  account_id: string;
  asked: string;
  views: Record<string, unknown>;
  missing_views: string[];
}

function state(name: string): AccountState {
  return JSON.parse(readFileSync(join(STATES, "../account-plan-states", `${name}.json`), "utf8")) as AccountState;
}

function readerFrom(s: AccountState, calls: { method: string; args: unknown }[] = []): ChainReader {
  return {
    async view<T>(method: string, args?: object): Promise<T> {
      calls.push({ method, args });
      if (s.missing_views.includes(method) || !(method in s.views)) throw new ViewNotAvailable(method);
      return s.views[method] as T;
    },
    async account() {
      throw new Error("not used");
    },
    async accessKeyCount() {
      throw new Error("not used");
    },
    async events() {
      throw new Error("not used");
    },
  };
}

async function show(name: string, edit: (s: AccountState) => void = () => {}) {
  const s = state(name);
  edit(s);
  const nowNs = BigInt(s.now_ns);
  const loaded = await loadAccountPlan(readerFrom(s), { accountId: s.account_id, asked: BigInt(s.asked) });
  return yourPlanView(
    { accountId: s.account_id, symbol: s.symbol, decimals: s.decimals, totalSupply: BigInt(s.total_supply), ...loaded },
    { nowNs, timeZone: "UTC" },
  );
}

describe("sell fills in part", () => {
  it("shows the plan and a preview with what fills, what stays and when more can follow", async () => {
    const { plan, preview, asOf } = await show("sell-fills-in-part");
    expect(asOf).toBe("As of 14:24");
    expect(plan).toMatchObject({
      kind: "card",
      accountId: "you.testnet",
      tags: [{ label: "Large" }, { label: "Cannot send" }],
      balance: "Balance 30.0M MOTH (3.0% of supply)",
      big: "7.1M",
      bigLabel: "can sell now",
      bigSub: null,
      usedLabel: "0 of 7.1M used",
      refill: "Window resets 15:02, in 38 min",
    });
    expect(preview).toMatchObject({
      title: "Your sell fills in part",
      tone: "limit",
      lines: [
        { label: "Asked", value: "30.0M MOTH" },
        { label: "Filled now", value: "7.1M MOTH (0.71%)" },
        { label: "Stays in wallet", value: "22.9M MOTH" },
        { label: "NEAR out", value: "1.23 NEAR" },
        { label: "Can sell more after", value: "15:02, in 38 min" },
      ],
      copy: "Fills 7.1M of 30.0M MOTH now. This is all your allowance lets through. 22.9M MOTH stays in your wallet. You can sell again after 15:02. The new allowance depends on your balance then.",
    });
  });

  it("asks the coin for the account and the amount, and for nothing else about the sell", async () => {
    const s = state("sell-fills-in-part");
    const calls: { method: string; args: unknown }[] = [];
    await loadAccountPlan(readerFrom(s, calls), { accountId: "you.testnet", asked: BigInt(s.asked) });
    expect(calls).toEqual([
      { method: "fc_account", args: { account_id: "you.testnet" } },
      { method: "fc_quote_sell", args: { account_id: "you.testnet", amount: s.asked } },
    ]);
  });
});

describe("sell fills fully", () => {
  it("shows a quiet plan and a preview with no limit text", async () => {
    const { plan, preview } = await show("sell-fills-fully");
    expect(plan).toEqual({
      kind: "small",
      accountId: "you.testnet",
      balance: "5.0M MOTH",
      text: "No limit applies to your bag.",
    });
    expect(preview).toMatchObject({
      title: "Your sell fills in full",
      tone: "plain",
      lines: [
        { label: "Asked", value: "2.0M MOTH" },
        { label: "NEAR out", value: "0.36 NEAR" },
      ],
      copy: null,
    });
    const text = allText({ plan, preview }).join(" ");
    expect(text).not.toMatch(/allowance|cap|floor|penalty|refill|limit applies to your sell/i);
  });
});

describe("band with penalty", () => {
  it("shows the penalty from the quote and the size above which it applies", async () => {
    const { plan, preview } = await show("band-with-penalty");
    expect(plan).toMatchObject({ kind: "card", tags: [{ label: "Mid-size" }, { label: "Can send" }], big: "7.1M" });
    expect(preview).toMatchObject({
      title: "Part of your sell pays a penalty",
      tone: "limit",
      lines: [
        { label: "Asked", value: "12.1M MOTH" },
        { label: "NEAR out", value: "2.02 NEAR" },
        { label: "Penalty", value: "0.09 NEAR", tone: "limit" },
      ],
      copy: "Sells above 7.1M MOTH pay a penalty. The penalty stays in the pool.",
    });
  });
});

describe("cap full floor", () => {
  it("shows that just the small floor fills and when more can follow", async () => {
    const { plan, preview } = await show("cap-full-floor");
    expect(plan).toMatchObject({ kind: "card", big: "100K", bigSub: "Allowance this window: 7.1M. It resets 15:02, in 38 min." });
    expect(preview).toMatchObject({
      title: "Pool cap is full. Your sell fills just a small floor",
      tone: "limit",
      lines: [
        { label: "Asked", value: "30.0M MOTH" },
        { label: "Filled now", value: "100K MOTH (0.01%)" },
        { label: "Stays in wallet", value: "29.9M MOTH" },
        { label: "NEAR out", value: "0.02 NEAR" },
        { label: "Can sell more after", value: "15:02, in 38 min" },
      ],
      copy: "Fills 100K of 30.0M MOTH now. The pool cap is full for this hour, so just a small floor fills. 29.9M MOTH stays in your wallet. You can sell again after 15:02. The new allowance depends on your balance then.",
    });
  });
});

describe("a small account at a full cap", () => {
  const HELD_BACK = "Profit from tokens you bought this hour is held back while the pool cap is full.";

  it("adds the held-back line when the view says it cannot sell its whole balance now", async () => {
    const { plan } = await show("sell-fills-fully", (s) => {
      (s.views.fc_account as Record<string, unknown>).max_full_fill_now = "4000000000000000000000000";
    });
    expect(plan).toEqual({
      kind: "small",
      accountId: "you.testnet",
      balance: "5.0M MOTH",
      text: `No limit applies to your bag. ${HELD_BACK}`,
    });
  });

  it("keeps the early-supply text for a small account with marked tokens", async () => {
    const { plan } = await show("sell-fills-fully", (s) => {
      const account = s.views.fc_account as Record<string, unknown>;
      account.max_full_fill_now = "4000000000000000000000000";
      account.marked_balance = "1000000000000000000000000";
    });
    expect(plan.kind).toBe("card");
    expect(allText(plan).join(" ")).not.toContain(HELD_BACK);
  });
});

describe("a band account below one allowance", () => {
  it("shows band and the penalty line from fc_account.class, not a small bag", async () => {
    const { plan } = await show("band-below-one-allowance");
    expect(plan).toMatchObject({ kind: "card", tags: [{ label: "Mid-size" }, { label: "Can send" }] });
    expect(allText(plan)).toContain("Sells above 7.1M MOTH in one window pay a penalty.");
    expect(allText(plan).join(" ")).not.toContain("No limit applies");
  });
});

describe("an early-supply holder", () => {
  it("reads the early-supply allowance and its 24 h reset from fc_account.marked", async () => {
    const { plan, preview } = await show("early-supply-holder");
    expect(plan).toMatchObject({
      kind: "card",
      tags: [{ label: "Early supply" }, { label: "Can send" }],
      big: "400K",
      usedLabel: "200K of 600K early supply used",
      refill: "Window resets 13:02 tomorrow, in 22 h 38 min",
      bigSub: "Allowance this window: 600K. It resets 13:02 tomorrow, in 22 h 38 min.",
    });
    expect(plan.kind === "card" && plan.usedPct).toBe(33);
    expect(preview).toMatchObject({
      lines: expect.arrayContaining([{ label: "Can sell more after", value: "13:02 tomorrow, in 22 h 38 min", tone: "limit" }]),
    });
    expect(allText(plan).join(" ")).not.toContain("15:02");
  });

  it("shows both resets when the account holds early supply and other tokens", async () => {
    const { plan, preview } = await show("early-supply-holder", (s) => {
      const a = s.views.fc_account as Record<string, unknown>;
      a.balance = "8000000000000000000000000";
      a.unmarked_balance = "3000000000000000000000000";
      a.unmarked = { allowance: "900000000000000000000000", used: "0", left: "900000000000000000000000", floor_left: "0", window_end_ns: "1794582120000000000" };
    });
    expect(plan).toMatchObject({
      refill: "Window resets 15:02, in 38 min",
      earlyLine: "Early supply: 400K of 600K left. It resets 13:02 tomorrow, in 22 h 38 min.",
    });
    expect(preview && preview.kind === "preview" && preview.lines.map((l) => [l.label, l.value])).toEqual(
      expect.arrayContaining([
        ["Early supply can sell more after", "13:02 tomorrow, in 22 h 38 min"],
        ["Other tokens can sell more after", "15:02, in 38 min"],
      ]),
    );
  });
});

describe("fields the coin does not return", () => {
  it("shows Not available from this coin, never a made-up value", async () => {
    const { plan } = await show("sell-fills-in-part", (s) => {
      const account = s.views.fc_account as Record<string, unknown>;
      delete account.max_receive;
      delete account.unmarked;
      delete account.max_full_fill_now;
    });
    expect(plan).toMatchObject({ kind: "card", big: NOT_AVAILABLE, usedLabel: NOT_AVAILABLE, refill: NOT_AVAILABLE });
    expect(allText(plan)).toContain(`Receive limit: ${NOT_AVAILABLE}`);
  });

  it("shows the whole plan as not available when the coin has no account view", async () => {
    const { plan, preview } = await show("sell-fills-in-part", (s) => {
      s.missing_views = ["fc_account", "fc_quote_sell"];
    });
    expect(plan).toEqual({ kind: "unavailable", accountId: "you.testnet", text: NOT_AVAILABLE });
    expect(preview).toEqual({ kind: "unavailable", text: NOT_AVAILABLE });
  });

  it("shows no preview until an amount is typed", async () => {
    const s = state("sell-fills-in-part");
    const loaded = await loadAccountPlan(readerFrom(s), { accountId: s.account_id, asked: null });
    const view = yourPlanView(
      { accountId: s.account_id, symbol: "MOTH", decimals: 18, totalSupply: BigInt(s.total_supply), ...loaded },
      { nowNs: BigInt(s.now_ns), timeZone: "UTC" },
    );
    expect(view.preview).toBeNull();
    expect(view.plan.kind).toBe("card");
  });
});

describe("copy rules", () => {
  it.each(["sell-fills-in-part", "sell-fills-fully", "band-with-penalty", "cap-full-floor"])("%s has no banned words", async (name) => {
    const view = await show(name);
    expect(allText(view).flatMap(copyViolations)).toEqual([]);
  });
});

describe("amount typed by the trader", () => {
  it.each([
    ["30000000", 18, 30_000_000n * 10n ** 18n],
    ["30,000,000", 18, 30_000_000n * 10n ** 18n],
    ["1.5", 18, 15n * 10n ** 17n],
    [" 0.000001 ", 18, 10n ** 12n],
  ])("reads %s", (text, decimals, want) => {
    expect(parseTokenAmount(text, decimals)).toBe(want);
  });

  it.each(["", "abc", "-1", "1.2.3", "0", "0.0000000000000000001"])("rejects %j", (text) => {
    expect(parseTokenAmount(text, 18)).toBeNull();
  });
});
