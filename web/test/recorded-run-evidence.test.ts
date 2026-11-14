import { spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// The real evidence file of our testnet run, through the page builder as a judge would run it.
const builder = new URL("../scripts/build-recorded-run.ts", import.meta.url).pathname;
const evidence = new URL("../../evidence/testnet-run.json", import.meta.url).pathname;
const buyers = Array.from({ length: 18 }, (_, i) => `bag-buyer-${String(i + 1).padStart(2, "0")}.fairchance.testnet`);

describe("the recorded testnet run from the evidence file", () => {
  it("builds 18 sells by bag-buyer-01 to -18 in order: 17 full and 1 partial by bag-buyer-17", () => {
    const dir = mkdtempSync(join(tmpdir(), "fc-evidence-"));
    try {
      const out = join(dir, "recorded-run.json");
      const r = spawnSync("node", [builder, evidence, out], { encoding: "utf8" });
      expect(r.status, r.stderr).toBe(0);
      const file = JSON.parse(readFileSync(out, "utf8")) as { transactions: Record<string, string>[] };
      expect(file.transactions.map((t) => t.account)).toEqual(buyers);
      const partial = file.transactions.filter((t) => t.outcome === "partial_fill").map((t) => t.account);
      expect(partial).toEqual(["bag-buyer-17.fairchance.testnet"]);
      expect(file.transactions.filter((t) => t.outcome === "full_fill")).toHaveLength(17);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("matches the committed page file byte for byte", () => {
    const dir = mkdtempSync(join(tmpdir(), "fc-evidence-"));
    try {
      const out = join(dir, "recorded-run.json");
      expect(spawnSync("node", [builder, evidence, out], { encoding: "utf8" }).status).toBe(0);
      const committed = new URL("../public/recorded-run.json", import.meta.url).pathname;
      expect(readFileSync(committed, "utf8")).toBe(readFileSync(out, "utf8"));
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
});
