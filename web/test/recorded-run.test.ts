import { describe, expect, it } from "vitest";
import { loadRecordedRun, parseRecordedRun } from "../src/recorded-run.ts";
import type { KnownNetwork } from "../src/known-deployments.ts";

// Clearly fake fixture data. This is the shape the page reads, not a real run.
const HASH_A = "1".repeat(44);
const HASH_B = "2".repeat(44);
const fixture = {
  network: "testnet",
  coin: "fake-demo.fake-factory.testnet",
  transactions: [
    { tx_hash: HASH_A, outcome: "partial_fill", account: "fake-buyer-1.testnet", tokens_asked: "5000000000000000000000000", tokens_sold: "1000000000000000000000000", near_out: "2500000000000000000000000" },
    { tx_hash: HASH_B, outcome: "refusal", account: "fake-buyer-2.testnet", refusal_code: "FC_EARLY_WINDOW" },
  ],
};
const explorer = "https://explorer.fake.example/txns/";

describe("reading a recorded run", () => {
  it("makes one row per transaction, in file order, with an explorer link from the fixed config", () => {
    const result = parseRecordedRun(fixture, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer });
    if (!result.ok) throw new Error(result.reason);
    expect(result.rows.map((r) => r.link)).toEqual([`${explorer}${HASH_A}`, `${explorer}${HASH_B}`]);
    expect(result.rows[0]!.text).toBe("fake-buyer-1.testnet sold 1.0M of 5.0M tokens for 2.50 NEAR. It filled in part.");
    expect(result.rows[1]!.text.startsWith("fake-buyer-2.testnet was refused.")).toBe(true);
    expect(result.rows[1]!.text).toContain("early");
  });

  it("refuses a file for another coin, so a wrong file never shows as this coin's run", () => {
    const result = parseRecordedRun({ ...fixture, coin: "other.fake-factory.testnet" }, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer });
    expect(result.ok).toBe(false);
  });

  it.each([
    ["a bad transaction hash", { tx_hash: "not a hash" }],
    ["an unknown outcome", { outcome: "magic" }],
    ["a refusal code with no text in the shared file", { outcome: "refusal", refusal_code: "FC_NOT_A_CODE" }],
    ["a fill with a non-numeric amount", { tokens_sold: "12.5" }],
  ])("refuses a file with %s", (_name, change) => {
    const bad = { ...fixture, transactions: [{ ...fixture.transactions[0]!, ...change }] };
    expect(parseRecordedRun(bad, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(false);
  });

  it("refuses a file that is not the shape, and an empty list", () => {
    expect(parseRecordedRun(null, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(false);
    expect(parseRecordedRun({ ...fixture, transactions: [] }, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(false);
  });

  it.each(["constructor", "__proto__", "toString", "hasOwnProperty"])("refuses the inherited key %s as a refusal code", (code) => {
    const bad = { ...fixture, transactions: [{ tx_hash: HASH_A, outcome: "refusal", account: "fake-buyer-2.testnet", refusal_code: code }] };
    expect(parseRecordedRun(bad, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(false);
  });

  it.each([
    ["a partial fill that sold more than it asked", { outcome: "partial_fill", tokens_asked: "100", tokens_sold: "900" }],
    ["a partial fill that sold exactly what it asked", { outcome: "partial_fill", tokens_asked: "100", tokens_sold: "100" }],
    ["a partial fill that sold nothing", { outcome: "partial_fill", tokens_asked: "100", tokens_sold: "0" }],
    ["a full fill that sold less than it asked", { outcome: "full_fill", tokens_asked: "900", tokens_sold: "100" }],
  ])("refuses the whole file for %s", (_name, change) => {
    const bad = { ...fixture, transactions: [{ ...fixture.transactions[0]!, ...change }] };
    expect(parseRecordedRun(bad, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(false);
  });

  it("accepts a full fill that sold exactly what it asked", () => {
    const good = { ...fixture, transactions: [{ ...fixture.transactions[0]!, outcome: "full_fill", tokens_asked: "100", tokens_sold: "100" }] };
    expect(parseRecordedRun(good, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(true);
  });

  it("refuses a file for another network, or with no network", () => {
    expect(parseRecordedRun({ ...fixture, network: "mainnet" }, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(false);
    const { network: _omitted, ...noNetwork } = fixture;
    expect(parseRecordedRun(noNetwork, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(false);
  });

  it("refuses a file that repeats a transaction hash", () => {
    const twice = { ...fixture, transactions: [fixture.transactions[0]!, fixture.transactions[0]!] };
    expect(parseRecordedRun(twice, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer }).ok).toBe(false);
  });

  it("never takes a link from the file", () => {
    const sneaky = { ...fixture, transactions: [{ ...fixture.transactions[0]!, explorer_url: "https://evil.example/x" }] };
    const result = parseRecordedRun(sneaky, { network: "testnet", coin: fixture.coin, explorerTxUrl: explorer });
    if (!result.ok) throw new Error(result.reason);
    expect(result.rows[0]!.link.startsWith(explorer)).toBe(true);
  });
});

const network = (file: string | null): KnownNetwork => ({ rpcUrl: "https://rpc.fake.example", fastNearUrl: "https://tx.fake.example", factoryFromBlock: 0, factory: null, codeHash: null, demoCoin: null, explorerTxUrl: explorer, recordedRunFile: file });
const link = `https://fc.example/?network=testnet&coin=${fixture.coin}`;
const reply = (body: unknown, status = 200) => async () => new Response(JSON.stringify(body), { status });

describe("loading the recorded run", () => {
  it("asks for nothing when the config has no file yet", async () => {
    let calls = 0;
    const result = await loadRecordedRun(link, network(null), async () => { calls++; return new Response("{}"); });
    expect(result).toBeNull();
    expect(calls).toBe(0);
  });

  it("reads the file the config names, on the same site", async () => {
    let asked = "";
    const result = await loadRecordedRun(link, network("evidence/testnet-run.json"), async (url) => { asked = String(url); return new Response(JSON.stringify(fixture)); });
    expect(asked).toBe("https://fc.example/evidence/testnet-run.json");
    expect(result?.ok).toBe(true);
  });

  it("reports a missing or broken file as a failure, never as an empty run", async () => {
    const missing = await loadRecordedRun(link, network("evidence/testnet-run.json"), reply({}, 404));
    expect(missing?.ok).toBe(false);
    const broken = await loadRecordedRun(link, network("evidence/testnet-run.json"), async () => new Response("<html>"));
    expect(broken?.ok).toBe(false);
  });

  it("reads nothing for a link with no network or no coin", async () => {
    expect(await loadRecordedRun("https://fc.example/", network("e.json"), reply(fixture))).toBeNull();
    expect(await loadRecordedRun("https://fc.example/?network=testnet", network("e.json"), reply(fixture))).toBeNull();
  });
});
