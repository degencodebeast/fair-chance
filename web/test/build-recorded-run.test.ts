import { describe, expect, it } from "vitest";
import { buildRecordedRun } from "../scripts/build-recorded-run.ts";
import { parseRecordedRun } from "../src/recorded-run.ts";

// Fake fixture shaped like the evidence file: more fields than the page reads.
const HASH_A = "1".repeat(44);
const HASH_B = "2".repeat(44);
const evidence = {
  network: "testnet",
  coin: "fake-coin.fake-factory.testnet",
  factory: "fake-factory.testnet",
  code_hash: "FakeHash",
  launch: { tx_hash: HASH_A, account: "fake-creator.testnet" },
  buys: [{ tx_hash: HASH_A, account: "fake-buyer-01.testnet", near_in: "1" }],
  transactions: [
    { tx_hash: HASH_A, outcome: "partial_fill", account: "fake-buyer-01.testnet", tokens_asked: "100", tokens_sold: "40", near_out: "5", fee_near: "9", extra: "x" },
    { tx_hash: HASH_B, outcome: "refusal", account: "fake-buyer-02.testnet", refusal_code: "FC_EARLY_WINDOW", note: "x" },
  ],
};
const explorerTxUrl = "https://explorer.fake.example/txns/";

describe("building the recorded run file", () => {
  it("keeps only the fields the page reads, in file order", () => {
    const out = buildRecordedRun(evidence);
    expect(Object.keys(out).sort()).toEqual(["coin", "network", "transactions"]);
    expect(out.transactions).toEqual([
      { tx_hash: HASH_A, outcome: "partial_fill", account: "fake-buyer-01.testnet", tokens_asked: "100", tokens_sold: "40", near_out: "5" },
      { tx_hash: HASH_B, outcome: "refusal", account: "fake-buyer-02.testnet", refusal_code: "FC_EARLY_WINDOW" },
    ]);
  });

  it("makes a file the page accepts", () => {
    const r = parseRecordedRun(buildRecordedRun(evidence), { network: "testnet", coin: evidence.coin, explorerTxUrl });
    expect(r.ok).toBe(true);
  });

  it("copies no buys, launch, key or hash field into the output", () => {
    const text = JSON.stringify(buildRecordedRun(evidence));
    for (const leak of ["buys", "launch", "code_hash", "fake-creator", "fee_near", "near_in"]) expect(text).not.toContain(leak);
  });

  it.each([
    ["no transactions yet", { ...evidence, transactions: [] }],
    ["a transaction the page would refuse", { ...evidence, transactions: [{ ...evidence.transactions[0]!, tokens_sold: "100" }] }],
    ["no coin", { ...evidence, coin: undefined }],
    ["an unknown outcome", { ...evidence, transactions: [{ ...evidence.transactions[0]!, outcome: "magic" }] }],
  ])("refuses evidence with %s, so a half-made run is never written", (_n, bad) => {
    expect(() => buildRecordedRun(bad)).toThrow();
  });
});
