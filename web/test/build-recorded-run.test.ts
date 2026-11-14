import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import { buildRecordedRun } from "../scripts/build-recorded-run.ts";
import { parseRecordedRun } from "../src/recorded-run.ts";

// Fake fixture shaped like the evidence file: more fields than the page reads.
const HASH_A = "1".repeat(44);
const HASH_B = "2".repeat(44);
const evidence = {
  network: "testnet",
  coin: KNOWN_NETWORKS.testnet.demoCoin,
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

// The real known network and coin, with one valid transaction.
const known = { network: "testnet", coin: KNOWN_NETWORKS.testnet.demoCoin, transactions: [evidence.transactions[0]!] };

describe("the network and coin of the evidence", () => {
  it("accepts the known network with its demo coin", () => {
    expect(buildRecordedRun(known).coin).toBe(KNOWN_NETWORKS.testnet.demoCoin);
  });

  it.each([
    ["a network that is a URL with a key", { ...known, network: "https://rpc.fake-private.example/?apikey=FAKESECRET" }],
    ["a network that is not a known network", { ...known, network: "mainnet" }],
    ["an inherited key as the network", { ...known, network: "constructor" }],
    ["a coin that is not the demo coin", { ...known, coin: "ed25519:FAKESECRET" }],
    ["another coin under the factory", { ...known, coin: "other.factory.fairchance.testnet" }],
  ])("refuses %s, so nothing private is copied into the page file", (_n, bad) => {
    expect(() => buildRecordedRun(bad)).toThrow();
  });
});

describe("the command line", () => {
  it("runs under plain Node and writes the file", () => {
    const dir = mkdtempSync(join(tmpdir(), "fc-brr-"));
    try {
      writeFileSync(join(dir, "ev.json"), JSON.stringify(known));
      const out = join(dir, "out.json");
      execFileSync("node", [new URL("../scripts/build-recorded-run.ts", import.meta.url).pathname, join(dir, "ev.json"), out], { encoding: "utf8" });
      expect(JSON.parse(readFileSync(out, "utf8"))).toEqual({ network: "testnet", coin: known.coin, transactions: [{ tx_hash: HASH_A, outcome: "partial_fill", account: "fake-buyer-01.testnet", tokens_asked: "100", tokens_sold: "40", near_out: "5" }] });
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("exits 1 and writes nothing for a bad network", () => {
    const dir = mkdtempSync(join(tmpdir(), "fc-brr-"));
    try {
      writeFileSync(join(dir, "ev.json"), JSON.stringify({ ...known, network: "https://rpc.fake-private.example/?apikey=FAKESECRET" }));
      const r = spawnSync("node", [new URL("../scripts/build-recorded-run.ts", import.meta.url).pathname, join(dir, "ev.json"), join(dir, "out.json")], { encoding: "utf8" });
      expect(r.status).toBe(1);
      expect(r.stderr).not.toContain("FAKESECRET");
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
