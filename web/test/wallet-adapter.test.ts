import { describe, expect, it } from "vitest";
import { connectorOptions, nearConnectLibrary, type ConnectorLike, type WalletLike } from "../src/wallet-adapter.ts";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import type { TxRequest } from "../src/tx-types.ts";

function fakeWallet(accounts: string[], testnet = true): WalletLike & { calls: unknown[] } {
  const calls: unknown[] = [];
  return {
    calls,
    manifest: { features: { testnet } },
    async getAccounts(data) {
      calls.push(["getAccounts", data]);
      return accounts.map((accountId) => ({ accountId }));
    },
    async signAndSendTransactions(params) {
      calls.push(["signAndSendTransactions", params]);
      return params.transactions.map(() => ({ transaction: { hash: "h" } }));
    },
  };
}

function fakeConnector(wallet: WalletLike | null, network = "testnet") {
  const handlers: Record<string, (p: any) => void> = {};
  const calls: string[] = [];
  const connector: ConnectorLike = {
    network,
    async connect() {
      calls.push("connect");
      if (wallet === null) throw new Error("User rejected");
      return wallet;
    },
    async wallet() {
      calls.push("wallet");
      if (wallet === null) throw new Error("No accounts found");
      return wallet;
    },
    async disconnect() {
      calls.push("disconnect");
    },
    on: (event: string, cb: (p: any) => void) => void (handlers[event] = cb),
  };
  return { connector, calls, handlers };
}

const TX: TxRequest = { receiverId: "bag.factory.fairchance.testnet", actions: [{ type: "FunctionCall", params: { methodName: "buy", args: {}, gas: "1", deposit: "1" } }] };

describe("near-connect adapter", () => {
  it("runs the library on testnet with only the public RPC from the fixed config", () => {
    const options = connectorOptions();
    expect(options.network).toBe("testnet");
    expect(options.providers).toEqual({ testnet: [KNOWN_NETWORKS.testnet.rpcUrl] });
    expect(options.features).toEqual({ testnet: true, signAndSendTransactions: true });
    expect(options.autoConnect).toBe(false);
  });

  it("reports the account and the testnet network after a connect", async () => {
    const w = fakeWallet(["lamp.testnet"]);
    const lib = nearConnectLibrary(fakeConnector(w).connector);
    expect(await lib.connect()).toEqual({ accountId: "lamp.testnet", network: "testnet" });
    expect(w.calls).toEqual([["getAccounts", { network: "testnet" }]]);
  });

  it("reports no network when the wallet does not say it works on testnet, or the library is on mainnet", async () => {
    expect(await nearConnectLibrary(fakeConnector(fakeWallet(["lamp.testnet"], false)).connector).connect()).toEqual({ accountId: "lamp.testnet", network: "" });
    expect(await nearConnectLibrary(fakeConnector(fakeWallet(["lamp.near"]), "mainnet").connector).connect()).toEqual({ accountId: "lamp.near", network: "mainnet" });
  });

  it("rejects a connect that gives no account", async () => {
    await expect(nearConnectLibrary(fakeConnector(fakeWallet([])).connector).connect()).rejects.toThrow();
  });

  it("restores the saved wallet's account, or nothing", async () => {
    expect(await nearConnectLibrary(fakeConnector(fakeWallet(["lamp.testnet"])).connector).restore()).toEqual({ accountId: "lamp.testnet", network: "testnet" });
    expect(await nearConnectLibrary(fakeConnector(null).connector).restore()).toBeNull();
  });

  it("sends on testnet as the signed-in account", async () => {
    const w = fakeWallet(["lamp.testnet"]);
    const outcomes = await nearConnectLibrary(fakeConnector(w).connector).send("lamp.testnet", [TX, TX]);
    expect(outcomes).toHaveLength(2);
    expect(w.calls).toEqual([["signAndSendTransactions", { network: "testnet", signerId: "lamp.testnet", transactions: [TX, TX] }]]);
  });

  it("passes the library's sign-in and sign-out events on", async () => {
    const f = fakeConnector(fakeWallet(["lamp.testnet"]));
    const lib = nearConnectLibrary(f.connector);
    const seen: unknown[] = [];
    lib.onSignIn((a) => seen.push(a));
    lib.onSignOut(() => seen.push("out"));
    f.handlers["wallet:signIn"]!({ wallet: fakeWallet(["wick.lamp.testnet"]), accounts: [{ accountId: "wick.lamp.testnet" }] });
    f.handlers["wallet:signOut"]!({});
    await lib.signOut();
    expect(seen).toEqual([{ accountId: "wick.lamp.testnet", network: "testnet" }, "out"]);
    expect(f.calls).toEqual(["disconnect"]);
  });
});
