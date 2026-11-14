import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { optionsFromConfig, scanBuildOutput } from "../scripts/scan-build-output.ts";

// Public testnet transaction hashes, as in the recorded run file.
const HASH_A = "EU7f9gmEmNdWGLjxnAHU3UTCxnZofAehN3jHUhebcJvD";
const HASH_B = "8EE8hVPRFp2YNRVUd1gVGKk9CkAjkTZoagv7aQDoHXkM";

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "fc-hash-"));
  mkdirSync(join(dir, "assets"));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

const rules = async () => (await scanBuildOutput(dir, optionsFromConfig({}))).map((f) => f.rule);
const run = (tx: object[]) => JSON.stringify({ network: "testnet", coin: "bag.factory.fairchance.testnet", transactions: tx });

describe("transaction hashes in the recorded run file", () => {
  it("passes the tx_hash values of the file", async () => {
    writeFileSync(join(dir, "recorded-run.json"), run([{ tx_hash: HASH_A, account: "bag-buyer-01.fairchance.testnet" }, { tx_hash: HASH_B }]));
    expect(await rules()).toEqual([]);
  });

  it("still fails the same hash in any other file", async () => {
    writeFileSync(join(dir, "recorded-run.json"), run([{ tx_hash: HASH_A }]));
    writeFileSync(join(dir, "assets", "app.js"), `const h="${HASH_A}";`);
    expect(await rules()).toEqual(["key"]);
  });

  it("still fails a key-like value in another field of the file", async () => {
    writeFileSync(join(dir, "recorded-run.json"), run([{ tx_hash: HASH_A, note: HASH_B }]));
    expect(await rules()).toEqual(["key"]);
  });

  it("still fails a long key-like value that is not a transaction hash length", async () => {
    writeFileSync(join(dir, "recorded-run.json"), run([{ tx_hash: HASH_A + "Zq7Wk" }]));
    expect((await rules()).includes("key")).toBe(true);
  });

  it("does not pass hashes from a file that is not valid JSON", async () => {
    writeFileSync(join(dir, "recorded-run.json"), `{ "transactions": [ { "tx_hash": "${HASH_A}" `);
    expect(await rules()).toEqual(["key"]);
  });
});
