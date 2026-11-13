import { describe, expect, it } from "vitest";
import { chainReader, ViewNotAvailable } from "../src/chain.ts";

function answering(result: object | null, error?: object): typeof fetch {
  return (async () => Response.json({ jsonrpc: "2.0", id: "fc", ...(error ? { error } : { result }) })) as unknown as typeof fetch;
}
const cfg = { rpcUrl: "http://127.0.0.1:1", coin: "walk.factory.sandbox" };

describe("account facts", () => {
  it("reads the global contract hash and the local code hash", async () => {
    const reader = chainReader(cfg, answering({ code_hash: "11111111111111111111111111111111", global_contract_hash: "H1", global_contract_account_id: null }));
    expect(await reader.account(cfg.coin)).toEqual({ globalContractHash: "H1", usesGlobalContractByAccount: false, localCodeHash: null });
  });

  it("reports a global contract used by account name", async () => {
    const reader = chainReader(cfg, answering({ code_hash: "11111111111111111111111111111111", global_contract_account_id: "owner.testnet" }));
    expect(await reader.account(cfg.coin)).toMatchObject({ globalContractHash: null, usesGlobalContractByAccount: true });
  });

  it("counts access keys", async () => {
    const reader = chainReader(cfg, answering({ keys: [{ public_key: "k", access_key: {} }] }));
    expect(await reader.accessKeyCount(cfg.coin)).toBe(1);
  });
});

describe("a view the coin does not have", () => {
  it("throws ViewNotAvailable for a missing method, in either answer shape", async () => {
    const inResult = chainReader(cfg, answering({ error: "wasm execution failed: MethodResolveError(MethodNotFound)", logs: [] }));
    await expect(inResult.view("fc_cap")).rejects.toBeInstanceOf(ViewNotAvailable);
    const inError = chainReader(
      cfg,
      answering(null, { name: "HANDLER_ERROR", cause: { name: "CONTRACT_EXECUTION_ERROR" }, data: "MethodResolveError(MethodNotFound)" }),
    );
    await expect(inError.view("fc_cap")).rejects.toBeInstanceOf(ViewNotAvailable);
  });

  it("does not hide another failure as not available", async () => {
    const reader = chainReader(cfg, answering({ error: "wasm execution failed: Smart contract panicked", logs: [] }));
    await expect(reader.view("fc_cap")).rejects.toThrow(/panicked/);
  });
});
