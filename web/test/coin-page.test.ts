import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import { describe, expect, it } from "vitest";
import { loadCoinPage } from "../src/coin-page.ts";
import { HttpStatusError } from "../src/refresher.ts";
import { coinPageModel, loadingFrame, NOT_AVAILABLE, type CoinPageModel } from "../src/coin-page-model.ts";
import { allText, copyViolations } from "./copy-rules.ts";
import { readerFrom, recordedCoin, type RecordedCoin } from "./recorded-coin.ts";

const coin = "moth.factory.testnet";
const factory = "factory.testnet";
const knownHash = "DmuJEGxMYTS5hjKvhyYpcJmoUBwHXmBRjCpWWDhbKXSa";
const baseHref = `http://localhost/?coin=${coin}&rpc=${encodeURIComponent("http://127.0.0.1:1")}&factory=${factory}&code_hash=${knownHash}`;

async function page(rec: RecordedCoin, href = baseHref) {
  const nowNs = BigInt(rec.now_ns);
  const data = await loadCoinPage(href, { reader: readerFrom(rec, coin), nowNs });
  return coinPageModel(data, { nowNs, timeZone: "UTC" });
}

async function ready(rec: RecordedCoin, href?: string): Promise<CoinPageModel> {
  const view = await page(rec, href);
  if (view.state !== "ready") throw new Error(`page is ${view.state}: ${JSON.stringify(view)}`);
  return view;
}

describe("early window live", () => {
  it("shows the early banner, the buy fee and the locked early supply", async () => {
    const view = await ready(recordedCoin("early-window-live"));
    expect(view.banner).toEqual({
      kind: "early",
      blocksLeft: 13,
      block: 5,
      end: 17,
      buyFee: "30.0%",
      feeEnds: "Fee ends at block 18, about 8 s",
      text: "Buys in the next 13 blocks (about 8 s) count as early supply. Early supply sells at one shared pace for the whole group.",
    });
    const early = view.who.find((c) => c.id === "early")!;
    expect(early.lines.map((l) => l.text)).toContain("Early supply cannot sell until block 17 has passed.");
    expect(view.stats.find((s) => s.label === "Age")).toMatchObject({ value: "7 s" });
  });
});

describe("normal", () => {
  it("fills the stats row from the pool view", async () => {
    const view = await ready(recordedCoin("normal"));
    expect(view.status).toBe("1 sell receipt from block 100, read from the chain.");
    expect(view.header).toMatchObject({
      name: "Moth Lamp",
      symbol: "MOTH",
      creator: "lamp.testnet",
      launched: "13:02, 1 h 22 min ago",
      description: "A lamp that remembers every moth.",
    });
    expect(view.stats).toEqual([
      { label: "Price per 1M MOTH", value: "0.1729 NEAR", sub: "", available: true },
      { label: "Market cap", value: "172.99 NEAR", sub: "", available: true },
      { label: "Real NEAR in pool", value: "40.47 NEAR", sub: "+ 100.00 virtual", available: true },
      { label: "Age", value: "1 h 22 min", sub: "launched 13:02", available: true },
    ]);
    expect(view.banner).toMatchObject({ kind: "ended" });
  });

  it("shows who can sell how fast from the pool, early window and cap views", async () => {
    const view = await ready(recordedCoin("normal"));
    const [early, large, cap] = view.who;
    expect(early!.lines.map((l) => l.text)).toEqual([
      "Creator group: 3.9% of supply, limit 4.0%",
      "Early supply: 18.4% of supply",
      "Shared allowance per 24 h window: 4.2M MOTH, for all early supply together",
    ]);
    expect(large!.lines[0]!.text).toBe("2 large holders now");
    expect(cap!.chip).toEqual({ text: "Has room", tone: "plain" });
    expect(cap!.meter).toEqual({ text: "Room left in the pool cap this hour: 1.23 of 2.13 NEAR", usedPct: 42, tone: "plain" });
    expect(cap!.lines.map((l) => l.text)).toContain("Resets 15:02, in 38 min");
  });

  it("labels the cap room as room for the whole pool and points to the quote for one account's limit", async () => {
    const view = await ready(recordedCoin("normal"));
    const cap = view.who[2]!;
    const text = [cap.meter!.text, ...cap.lines.map((l) => l.text)];
    expect(text.flatMap((t) => copyViolations(t))).toEqual([]);
    expect(cap.meter!.text).toBe("Room left in the pool cap this hour: 1.23 of 2.13 NEAR");
    expect(cap.lines.map((l) => l.text)).toContain("This room is for the whole pool, not for one account. Your quote shows what your sell fills.");
    expect(text.filter((t) => /you can sell/i.test(t))).toEqual([]);
    expect(view.summaryLines[2]).toEqual({ label: "Pool cap", value: "1.23 of 2.13 NEAR room left, resets 15:02", tone: "plain" });
  });

  it("tells a trader that selling tokens bought this hour counts toward the cap", async () => {
    const view = await ready(recordedCoin("normal"));
    const cap = view.who[2]!;
    expect(cap.lines.flatMap((l) => copyViolations(l.text))).toEqual([]);
    expect(cap.lines.map((l) => l.text)).toContain(
      "Selling tokens you bought this hour counts toward the cap. When the cap has too little room, profit from them is held back.",
    );
  });

  it("lists the receipts newest first with local time and a relative hint", async () => {
    const view = await ready(recordedCoin("normal"));
    expect(view.receipts.map((r) => [r.kind, r.time])).toEqual([
      ["Sell", "14:21, 3 min ago"],
      ["Became large", "14:09, 15 min ago"],
    ]);
    expect(view.receipts[0]).toMatchObject({
      sellReceiptId: "SellReceipt111111111111111111111111111111111",
      block: 2200,
      sentence: "alice.testnet sold 500K of 500K tokens for 0.30 NEAR. It filled in full.",
      detail: null,
    });
    expect(view.receipts[1]!.sentence).toBe(
      "bigfish.testnet now holds 34.0M tokens (3.4% of supply). It is large and cannot send tokens.",
    );
    expect(view.receipts[1]!.sellReceiptId).toBeNull();
  });

  it("shows the rules from fc_rules with the code hash and the launch description", async () => {
    const view = await ready(recordedCoin("normal"));
    expect(view.codeHash).toBe(knownHash);
    expect(view.rules).toContainEqual({ label: "Pool cap per hour", value: "5.0% of real NEAR" });
    expect(view.rules).toContainEqual({ label: "Large from", value: "2.0% of supply" });
    expect(view.rules).toContainEqual({ label: "Creator group holds at most", value: "4.0% of supply" });
    expect(view.rulesStatement).toMatch(/^Nobody can change these rules/);
    expect(view.warning).toBeNull();
  });

  it("shows the creator share of the swap fee that fc_rules reports, which is the share the coin applies", async () => {
    const creatorShare = (view: CoinPageModel & { state: "ready" }) =>
      view.rules.find((r) => r.label === "Creator share of the swap fee")?.value;
    expect(creatorShare(await ready(recordedCoin("normal")))).toBe("30.0%");
    const rec = recordedCoin("normal");
    const rules = rec.views.fc_rules as Record<string, unknown>;
    rec.views.fc_rules = { ...rules, creator_fee_share_bps: 0 };
    expect(creatorShare(await ready(rec))).toBe("0.0%");
  });

  it("marks the receipt named in the page link as from the alert", async () => {
    const href = `${baseHref}&receipt=SellReceipt111111111111111111111111111111111`;
    const view = await ready(recordedCoin("normal"), href);
    expect(view.receipts.map((r) => r.fromAlert)).toEqual([true, false]);
  });
});

describe("values read at a time", () => {
  const later = async (state: string, afterNs: bigint) => {
    const rec = recordedCoin(state);
    const data = await loadCoinPage(baseHref, { reader: readerFrom(rec, coin), nowNs: BigInt(rec.now_ns) });
    const view = coinPageModel(data, { nowNs: BigInt(rec.now_ns) + afterNs, timeZone: "UTC" });
    if (view.state !== "ready") throw new Error("not ready");
    return view;
  };

  it("shows the time the values were read", async () => {
    const view = await ready(recordedCoin("normal"));
    expect(view.asOf).toBe("As of 14:24");
  });

  it("shows the cap as live before the hour ends", async () => {
    const view = await later("normal", 60n * 1_000_000_000n);
    const cap = view.who.find((c) => c.id === "cap")!;
    expect(cap.chip).toEqual({ text: "Has room", tone: "plain" });
    expect(cap.meter).not.toBeNull();
    expect(view.refreshAtNs).toBe(1794582120000000000n);
  });

  it("marks the cap values old after the hour ends and shows no live figure", async () => {
    const view = await later("normal", 40n * 60n * 1_000_000_000n);
    const cap = view.who.find((c) => c.id === "cap")!;
    expect(cap.chip).toEqual({ text: "Old", tone: "limit" });
    expect(cap.meter).toBeNull();
    expect(cap.lines.map((l) => l.text)).toEqual(["Read at 14:24 for the hour that ended at 15:02. Reload to read the new hour."]);
    expect(view.summaryLines.find((l) => l.label === "Pool cap")).toEqual({ label: "Pool cap", value: "Old, reload", tone: "limit" });
  });

  it("marks the early banner old when the block count it showed is over", async () => {
    const fresh = await later("early-window-live", 1_000_000_000n);
    expect(fresh.banner).toMatchObject({ kind: "early", blocksLeft: 13 });
    const old = await later("early-window-live", 20n * 1_000_000_000n);
    expect(old.banner).toEqual({ kind: "old", text: "The early window count was read at 14:24 and may be over. Reload to read it." });
    expect(fresh.refreshAtNs).not.toBeNull();
  });
});

describe("cap full", () => {
  it("shows the cap as full and a limited sell with its limit and next sell time", async () => {
    const view = await ready(recordedCoin("cap-full"));
    const cap = view.who.find((c) => c.id === "cap")!;
    expect(cap.chip).toEqual({ text: "Full", tone: "limit" });
    expect(cap.meter).toEqual({ text: "Room left in the pool cap this hour: 0.00 of 2.13 NEAR", usedPct: 100, tone: "limit" });
    expect(cap.lines.map((l) => l.text)).toContain("Large and early-supply sells fill just a small floor until 15:02.");
    expect(view.receipts.map((r) => r.kind)).toEqual(["Partial fill", "Cap full"]);
    expect(view.receipts[0]).toMatchObject({
      tone: "limit",
      detail: "Limit: the pool cap is full for this hour. It can sell more after 15:02, in 38 min.",
    });
    expect(view.receipts[1]!.sentence).toBe(
      "Pool cap full for this hour: 2.13 NEAR out. Large and early-supply sells fill just a small floor until 15:02.",
    );
    expect(view.summaryLines.find((l) => l.label === "Pool cap")).toEqual({ label: "Pool cap", value: "full, resets 15:02", tone: "limit" });
  });
});

describe("empty receipts", () => {
  it("says there are no sells yet and lists nothing", async () => {
    const view = await ready(recordedCoin("empty-receipts"));
    expect(view.receipts).toEqual([]);
    expect(view.receiptsEmpty).toMatch(/^No sells yet\./);
  });
});

describe("RPC error", () => {
  it("keeps the HTTP status of a failed read, so a refresh can back off", async () => {
    const rec = recordedCoin("normal");
    const failing = { ...readerFrom(rec, coin), view: async () => { throw new HttpStatusError("fc_rules: HTTP 429", 429, 9000); } };
    const data = await loadCoinPage(baseHref, { reader: failing, nowNs: BigInt(rec.now_ns) });
    expect(data.kind).toBe("error");
    if (data.kind === "error") expect([data.failure?.status, data.failure?.retryAfterMs]).toEqual([429, 9000]);
  });

  it("shows an error and no number, so nothing stale looks live", async () => {
    const view = await page(recordedCoin("rpc-error"));
    expect(view).toEqual({
      state: "error",
      status: "Values cannot load: fc_rules: HTTP 503",
      title: "Values cannot load from the local sandbox.",
      frame: loadingFrame("Not loaded"),
      detail: "Nothing on this page is shown as live until they load. fc_rules: HTTP 503",
      lastAttempt: "14:24",
    });
  });

  it("shows an error when a later read fails, with no partial numbers", async () => {
    const rec = recordedCoin("normal");
    const reader = readerFrom(rec, coin);
    const failing = { ...reader, events: async () => { throw new Error("the node no longer has the blocks"); } };
    const nowNs = BigInt(rec.now_ns);
    const data = await loadCoinPage(baseHref, { reader: failing, nowNs });
    const view = coinPageModel(data, { nowNs, timeZone: "UTC" });
    expect(view.state).toBe("error");
    expect(JSON.stringify(view)).not.toMatch(/\d NEAR/);
  });
});

describe("views the coin does not return", () => {
  it("shows 'not available from this coin' and no made-up value", async () => {
    const rec = recordedCoin("normal");
    rec.missing_views = ["fc_pool", "fc_cap", "ft_total_supply"];
    const view = await ready(rec);
    expect(view.stats.slice(0, 3).map((s) => [s.value, s.available])).toEqual([
      [NOT_AVAILABLE, false],
      [NOT_AVAILABLE, false],
      [NOT_AVAILABLE, false],
    ]);
    const [early, large, cap] = view.who;
    expect(early!.lines[0]!.text).toBe(NOT_AVAILABLE);
    expect(large!.lines[0]!.text).toBe(NOT_AVAILABLE);
    expect(cap!.lines).toEqual([{ text: NOT_AVAILABLE, tone: "plain" }]);
    expect(cap!.meter).toBeNull();
  });
});

describe("coin account checks", () => {
  it("shows a warning when the coin is not a direct sub-account of the known factory", async () => {
    const href = baseHref.replace(`factory=${factory}`, "factory=other.testnet");
    const view = await ready(recordedCoin("normal"), href);
    expect(view.warning?.title).toBe("Warning: this account does not match the Fair Chance launch rules.");
    expect(view.warning?.reasons).toEqual([
      "Coin is a direct sub-account of the factory: moth.factory.testnet is not a direct sub-account of other.testnet.",
    ]);
    expect(view.rulesStatement).toBeNull();
  });

  it("shows a warning when the global contract hash differs from the known hash", async () => {
    const href = baseHref.replace(knownHash, "SomeOtherHash111111111111111111111111111111");
    const view = await ready(recordedCoin("normal"), href);
    expect(view.warning?.reasons[0]).toMatch(/^Coin runs the known code by hash: The account runs hash DmuJ.*, not the known hash SomeOtherHash/);
  });

  it("shows a warning when the coin has an access key", async () => {
    const rec = recordedCoin("normal");
    rec.access_key_count = 1;
    const view = await ready(rec);
    expect(view.warning?.reasons).toEqual(["Coin has no access keys: The account has 1 access key."]);
  });

  it("shows a warning when the coin runs a global contract by account name", async () => {
    const rec = recordedCoin("normal");
    rec.account = { code_hash: "11111111111111111111111111111111", global_contract_hash: null, global_contract_account_id: "owner.testnet" };
    const view = await ready(rec);
    expect(view.warning?.reasons[0]).toMatch(/by account name/);
  });

  it("shows a warning, not a pass, when the link gives no known factory or hash", async () => {
    const view = await ready(recordedCoin("normal"), `http://localhost/?coin=${coin}&rpc=${encodeURIComponent("http://127.0.0.1:1")}`);
    expect(view.warning?.title).toBe("Warning: this page could not confirm that this account is a Fair Chance coin.");
    expect(view.checks.map((c) => c.status)).toEqual(["unchecked", "unchecked", "pass"]);
  });

  it("does not take a nested account for a direct sub-account", async () => {
    const href = baseHref.replace(`coin=${coin}`, `coin=x.${coin}`);
    const rec = recordedCoin("normal");
    const nowNs = BigInt(rec.now_ns);
    const data = await loadCoinPage(href, { reader: readerFrom(rec, `x.${coin}`), nowNs });
    const view = coinPageModel(data, { nowNs, timeZone: "UTC" });
    expect(view.state === "ready" && view.checks[0]!.status).toBe("fail");
  });
});

describe("copy rules", () => {
  it.each(["early-window-live", "normal", "cap-full", "empty-receipts", "rpc-error"])(
    "the %s page has no banned word, em dash or emoji",
    async (state) => {
      const view = await page(recordedCoin(state));
      const broken = allText(view).flatMap((text) => copyViolations(text).map((v) => `${v}: ${text}`));
      expect(broken).toEqual([]);
    },
  );

  it("the check finds a banned word", () => {
    expect(copyViolations("This coin is safe.")).toEqual(['banned word "safe"']);
    expect(copyViolations("Fills only a floor, the first time — now")).toEqual([
      'banned word "first"',
      'banned word "only"',
      "em dash",
    ]);
    expect(copyViolations("Pool cap safeguard")).toEqual([]);
  });
});

describe("known deployment comes from the page code, not the link", () => {
  const anchors = `&factory=${factory}&code_hash=${knownHash}`;

  it("does not let a crafted testnet link replace the fixed factory and hash", async () => {
    const href = `http://localhost/?coin=${coin}&network=testnet${anchors}`;
    const view = await ready(recordedCoin("normal"), href);
    expect(view.checks.map((c) => [c.id, c.status])).toEqual([
      ["parent-account", "fail"],
      ["code-hash", "fail"],
      ["no-access-keys", "pass"],
    ]);
    expect(view.checks[0]!.detail).toContain(KNOWN_NETWORKS.testnet.factory);
    expect(view.rulesStatement).toBeNull();
  });

  it.each(["http://localhost.evil.example:1", "https://rpc.testnet.evil.example"])(
    "loads nothing from the link RPC %s, even with anchors in the link",
    async (rpc) => {
      const href = `http://localhost/?coin=${coin}&rpc=${encodeURIComponent(rpc)}${anchors}`;
      const view = await page(recordedCoin("normal"), href);
      expect(view.state).toBe("error");
    },
  );

  it("takes link anchors on localhost and on 127.0.0.1, where each run deploys a fresh sandbox", async () => {
    for (const host of ["http://localhost:3030", "http://127.0.0.1:3030"]) {
      const href = `http://localhost/?coin=${coin}&rpc=${encodeURIComponent(host)}${anchors}`;
      const view = await ready(recordedCoin("normal"), href);
      expect(view.checks.map((c) => c.status)).toEqual(["pass", "pass", "pass"]);
    }
  });

  it("names the RPC host the checks used", async () => {
    const sandbox = await ready(recordedCoin("normal"));
    expect(sandbox.checksSource).toBe("Checks read through the RPC host 127.0.0.1:1.");
    const testnet = await ready(recordedCoin("normal"), `http://localhost/?coin=${coin}&network=testnet`);
    expect(testnet.checksSource).toBe("Checks read through the RPC host test.rpc.fastnear.com.");
  });
});

describe("storage bounds", () => {
  it("reads storage_balance_bounds().min from the coin", async () => {
    const rec = recordedCoin("normal");
    rec.views.storage_balance_bounds = { min: "1250000000000000000000", max: null };
    const nowNs = BigInt(rec.now_ns);
    const data = await loadCoinPage(baseHref, { reader: readerFrom(rec, coin), nowNs });
    if (data.kind !== "ready") throw new Error("page is not ready");
    expect(data.reads.storageMin).toEqual({ kind: "value", value: 1_250_000_000_000_000_000_000n });
  });

  it("says unavailable when the coin has no such view", async () => {
    const rec = recordedCoin("normal");
    delete rec.views.storage_balance_bounds;
    const data = await loadCoinPage(baseHref, { reader: readerFrom(rec, coin), nowNs: BigInt(rec.now_ns) });
    if (data.kind !== "ready") throw new Error("page is not ready");
    expect(data.reads.storageMin).toEqual({ kind: "unavailable" });
  });
});

describe("a coin name made only of removed characters", () => {
  it("shows the coin account as the page heading", async () => {
    const rec = recordedCoin("normal");
    rec.views.ft_metadata = { ...(rec.views.ft_metadata as object), name: "‮", symbol: "‏" };
    const view = await ready(rec);
    expect(view.header.name).toBe(coin);
  });
});
