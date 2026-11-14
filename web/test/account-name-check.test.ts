import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import { describe, expect, it, vi } from "vitest";
import { checkAccountName, launchNameCheckFromLink } from "../src/account-name-check.ts";

const reply = (body: object, status = 200) => vi.fn(async () => new Response(JSON.stringify(body), { status })) as unknown as typeof fetch;

describe("account name check", () => {
  it("reads the account with view_account through the given RPC and says taken when it exists", async () => {
    const f = reply({ jsonrpc: "2.0", id: "fc", result: { code_hash: "11111111111111111111111111111111", amount: "1" } });
    expect(await checkAccountName({ rpcUrl: "http://127.0.0.1:3030", accountId: "bag.factory.sandbox" }, f)).toBe("taken");
    const [url, init] = (f as unknown as ReturnType<typeof vi.fn>).mock.calls[0]! as [string, RequestInit];
    expect(url).toBe("http://127.0.0.1:3030");
    expect(JSON.parse(String(init.body))).toMatchObject({ method: "query", params: { request_type: "view_account", finality: "final", account_id: "bag.factory.sandbox" } });
  });

  it("says free when the node says the account does not exist, in either error form", async () => {
    const cfg = { rpcUrl: "http://127.0.0.1:3030", accountId: "bag.factory.sandbox" };
    expect(await checkAccountName(cfg, reply({ error: { name: "HANDLER_ERROR", cause: { name: "UNKNOWN_ACCOUNT" }, data: "x" } }))).toBe("free");
    expect(await checkAccountName(cfg, reply({ result: { error: "account bag.factory.sandbox does not exist while viewing" } }))).toBe("free");
  });

  it("throws for any other answer, so the form never shows free by mistake", async () => {
    const cfg = { rpcUrl: "http://127.0.0.1:3030", accountId: "bag.factory.sandbox" };
    await expect(checkAccountName(cfg, reply({}, 503))).rejects.toThrow(/HTTP 503/);
    await expect(checkAccountName(cfg, reply({ error: { cause: { name: "INTERNAL_ERROR" }, data: "boom" } }))).rejects.toThrow();
    await expect(checkAccountName(cfg, reply({ result: {} }))).rejects.toThrow();
  });
});

describe("launch name check from the page link", () => {
  it("uses the factory and RPC of a local sandbox link", () => {
    const c = launchNameCheckFromLink("http://localhost/?view=launch&rpc=http%3A%2F%2F127.0.0.1%3A3030&factory=factory.sandbox");
    expect(c.factory).toBe("factory.sandbox");
    expect(c.checkName).not.toBeNull();
  });
  it("uses the fixed testnet factory for a testnet link", () => {
    const c = launchNameCheckFromLink("http://localhost/?view=launch&network=testnet");
    expect(c.factory).toBe(KNOWN_NETWORKS.testnet.factory);
    expect(c.checkName).not.toBeNull();
  });
  it("checks nothing when the link names no factory or network, or a host that is not ours", () => {
    expect(launchNameCheckFromLink("http://localhost/?view=launch").checkName).toBeNull();
    expect(launchNameCheckFromLink("http://localhost/?view=launch&rpc=https%3A%2F%2Fevil.example").checkName).toBeNull();
  });
});
