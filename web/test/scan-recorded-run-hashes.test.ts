import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { evidenceTxHashes, optionsFromConfig, scanBuildOutput } from "../scripts/scan-build-output.ts";

// HASH_A and HASH_B are public testnet hashes from the evidence file. SEED is a random value that is in no evidence file.
const HASH_A = "EU7f9gmEmNdWGLjxnAHU3UTCxnZofAehN3jHUhebcJvD";
const HASH_B = "8EE8hVPRFp2YNRVUd1gVGKk9CkAjkTZoagv7aQDoHXkM";
const SEED = "Fq3Zs8KmP2xVd7RtYb9WnHc4JuLe6AaGg5Tk1QoXiDvN";

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "fc-hash-"));
  mkdirSync(join(dir, "assets"));
  mkdirSync(join(dir, "sub"));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

const options = () => ({ ...optionsFromConfig({}), evidenceTxHashes: new Set([HASH_A, HASH_B]) });
const rules = async () => (await scanBuildOutput(dir, options())).map((f) => f.rule);
const write = (name: string, text: string) => writeFileSync(join(dir, name), text);
const run = (body: string) => `{"network":"testnet","coin":"bag.factory.fairchance.testnet","transactions":[${body}]}`;

describe("transaction hashes in the recorded run file", () => {
  it("passes a tx_hash that is in the evidence", async () => {
    write("recorded-run.json", run(`{"tx_hash":"${HASH_A}","account":"bag-buyer-01.fairchance.testnet"},{"tx_hash":"${HASH_B}"}`));
    expect(await rules()).toEqual([]);
  });

  it("fails a seed value as the tx_hash", async () => {
    write("recorded-run.json", run(`{"tx_hash":"${SEED}"}`));
    expect(await rules()).toEqual(["key"]);
  });

  it("fails a seed value in a note field even when it is also the tx_hash", async () => {
    write("recorded-run.json", run(`{"tx_hash":"${SEED}","note":"${SEED}"}`));
    expect(await rules()).toEqual(["key", "key"]);
  });

  it("fails an evidence hash anywhere but the tx_hash field", async () => {
    write("recorded-run.json", run(`{"tx_hash":"${HASH_A}","note":"${HASH_A}"}`));
    expect(await rules()).toEqual(["key"]);
    write("recorded-run.json", run(`{"tx_hash":"${HASH_A}"}`).replace('"coin"', `"extra":["${HASH_B}"],"coin"`));
    expect(await rules()).toEqual(["key"]);
  });

  it("fails a seed value in an earlier duplicate tx_hash key", async () => {
    write("recorded-run.json", run(`{"tx_hash":"${SEED}","tx_hash":"${HASH_A}"}`));
    expect(await rules()).toEqual(["key"]);
  });

  it("fails a seed value written with a unicode escape", async () => {
    write("recorded-run.json", run(`{"tx_hash":"\\u0046${SEED.slice(1)}"}`));
    expect(await rules()).toEqual(["key"]);
  });

  it("fails an evidence hash written with a unicode escape in the tx_hash", async () => {
    write("recorded-run.json", run(`{"tx_hash":"\\u0045${HASH_A.slice(1)}"}`));
    expect(await rules()).toEqual(["key"]);
  });

  it("scans a recorded-run.json in a sub folder like any other file", async () => {
    write("sub/recorded-run.json", run(`{"tx_hash":"${HASH_A}"}`));
    expect(await rules()).toEqual(["key"]);
  });

  it("fails the same hash in any other file", async () => {
    write("assets/app.js", `const h="${HASH_A}";`);
    expect(await rules()).toEqual(["key"]);
  });

  it("fails every hash when the file is not valid JSON", async () => {
    write("recorded-run.json", `{ "transactions": [ { "tx_hash": "${HASH_A}" `);
    expect(await rules()).toEqual(["key"]);
  });
});

describe("the evidence hashes", () => {
  it("are read from the committed evidence files and hold the tx and tx_hash values only", () => {
    const hashes = evidenceTxHashes();
    expect(hashes.has(HASH_A)).toBe(true);
    expect(hashes.has(SEED)).toBe(false);
    expect(hashes.has("6Ld5DFTJ1BYc6uzb6YYvdK3yAMHdfBsvrwNEhsKE9er3")).toBe(false); // the code hash is not a transaction
  });
  it("come with the config options", () => {
    expect(optionsFromConfig({}).evidenceTxHashes?.has(HASH_A)).toBe(true);
  });
});
