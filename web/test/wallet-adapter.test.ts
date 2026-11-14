import { describe, expect, it } from "vitest";
import { connectorOptions, nearConnectLibrary, type ConnectorLike, type WalletLike } from "../src/wallet-adapter.ts";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import type { TxRequest } from "../src/tx-types.ts";
import type { WalletAccount } from "../src/wallet-session.ts";

function fakeWallet(id: string, accounts: unknown, testnet = true): WalletLike & { calls: unknown[] } {
  const calls: unknown[] = [];
  return {
    calls,
    manifest: { id, features: { testnet } },
    async getAccounts(data) {
      calls.push(["getAccounts", data]);
      return accounts as Array<{ accountId: string }>;
    },
    async signAndSendTransactions(params) {
      calls.push(["signAndSendTransactions", params]);
      return params.transactions.map(() => ({ transaction: { hash: "h" } }));
    },
    async signOut(data) {
      calls.push(["signOut", data]);
    },
  };
}

const ids = (...names: string[]) => names.map((accountId) => ({ accountId }));

/** A fake connector. `selected` is the wallet it has saved; connect() picks `picks` and emits the sign-in, as near-connect does. */
function fakeConnector(picks: WalletLike | null, network = "testnet") {
  const handlers: Record<string, (p: any) => void> = {};
  const calls: unknown[] = [];
  let selected: WalletLike | null = null;
  const connector: ConnectorLike = {
    network,
    async connect() {
      calls.push("connect");
      if (picks === null) throw new Error("User rejected");
      selected = picks;
      handlers["wallet:signIn"]?.({ wallet: picks, accounts: await picks.getAccounts({ network: "testnet" }) });
      return picks;
    },
    async wallet() {
      calls.push("wallet");
      if (selected === null) throw new Error("No accounts found");
      return selected;
    },
    async disconnect(wallet) {
      calls.push(["disconnect", wallet?.manifest.id]);
      selected = null;
    },
    on: (event: string, cb: (p: any) => void) => void (handlers[event] = cb),
  };
  return { connector, calls, handlers, select: (w: WalletLike) => void (selected = w) };
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

  it("reports the account, the testnet network and the wallet after a connect", async () => {
    const w = fakeWallet("A", ids("lamp.testnet"));
    const lib = nearConnectLibrary(fakeConnector(w).connector);
    expect(await lib.connect()).toEqual({ accountId: "lamp.testnet", network: "testnet", wallet: w });
    expect(w.calls).toContainEqual(["getAccounts", { network: "testnet" }]);
  });

  it("reports no network when the wallet does not say it works on testnet, or the library is on mainnet", async () => {
    expect((await nearConnectLibrary(fakeConnector(fakeWallet("A", ids("lamp.testnet"), false)).connector).connect()).network).toBe("");
    expect((await nearConnectLibrary(fakeConnector(fakeWallet("A", ids("lamp.near")), "mainnet").connector).connect()).network).toBe("mainnet");
  });

  it("passes on an answer with no account as an account the guard refuses, with its wallet", async () => {
    for (const accounts of [[], null, "lamp.testnet", [{}], [null]]) {
      const w = fakeWallet("A", accounts);
      const account = await nearConnectLibrary(fakeConnector(w).connector).connect();
      expect(typeof account.accountId, JSON.stringify(accounts)).not.toBe("string");
      expect(account.wallet).toBe(w);
    }
  });

  it("restores the saved wallet's account, or nothing", async () => {
    const w = fakeWallet("A", ids("lamp.testnet"));
    const f = fakeConnector(null);
    f.select(w);
    expect(await nearConnectLibrary(f.connector).restore()).toEqual({ accountId: "lamp.testnet", network: "testnet", wallet: w });
    expect(await nearConnectLibrary(fakeConnector(null).connector).restore()).toBeNull();
  });

  it("sends on testnet through the wallet that gave the account, as that account", async () => {
    const a = fakeWallet("A", ids("lamp.testnet"));
    const b = fakeWallet("B", ids("wick.testnet"));
    const f = fakeConnector(a);
    f.select(a);
    const lib = nearConnectLibrary(f.connector);
    const outcomes = await lib.send({ accountId: "wick.testnet", network: "testnet", wallet: b }, [TX, TX]);
    expect(outcomes).toHaveLength(2);
    expect(b.calls).toEqual([["signAndSendTransactions", { network: "testnet", signerId: "wick.testnet", transactions: [TX, TX] }]]);
    expect(a.calls).toEqual([]);
    await expect(lib.send({ accountId: "lamp.testnet", network: "testnet" }, [TX])).rejects.toThrow();
  });

  it("signs out the wallet that gave the account", async () => {
    const a = fakeWallet("A", ids("lamp.testnet"));
    const f = fakeConnector(a);
    const lib = nearConnectLibrary(f.connector);
    const account = await lib.connect();
    await lib.signOut(account);
    expect(f.calls).toEqual(["connect", ["disconnect", "A"]]);
  });

  it("signs out only a wallet that is not the selected one, and keeps the selected one", async () => {
    const a = fakeWallet("A", ids("lamp.testnet"));
    const b = fakeWallet("B", ids("lamp.near"));
    const f = fakeConnector(a);
    const lib = nearConnectLibrary(f.connector);
    await lib.connect();
    await lib.signOut({ accountId: "lamp.near", network: "testnet", wallet: b });
    expect(b.calls).toEqual([["signOut", { network: "testnet" }]]);
    expect(f.calls).toEqual(["connect"]);
  });

  it("passes on a sign-in from the selected wallet, and refuses one from another wallet", async () => {
    const a = fakeWallet("A", ids("lamp.testnet"));
    const b = fakeWallet("B", ids("carol.testnet"));
    const f = fakeConnector(a);
    const lib = nearConnectLibrary(f.connector);
    const seen: WalletAccount[] = [];
    lib.onSignIn((acc) => seen.push(acc));
    await lib.connect();
    f.handlers["wallet:signIn"]!({ wallet: a, accounts: ids("wick.lamp.testnet") });
    f.handlers["wallet:signIn"]!({ wallet: b, accounts: ids("carol.testnet") });
    await new Promise((r) => setTimeout(r, 0));
    expect(seen).toEqual([{ accountId: "wick.lamp.testnet", network: "testnet", wallet: a }]);
    expect(b.calls).toEqual([["signOut", { network: "testnet" }]]);
    expect(f.calls).toEqual(["connect"]);
  });

  it("does not pass on the sign-in event that its own connect makes", async () => {
    const f = fakeConnector(fakeWallet("A", ids("lamp.testnet")));
    const lib = nearConnectLibrary(f.connector);
    const seen: unknown[] = [];
    lib.onSignIn((acc) => seen.push(acc));
    await lib.connect();
    expect(seen).toEqual([]);
  });

  it("passes on a malformed sign-in from the selected wallet as an account the guard refuses", async () => {
    const a = fakeWallet("A", ids("lamp.testnet"));
    const f = fakeConnector(a);
    const lib = nearConnectLibrary(f.connector);
    const seen: WalletAccount[] = [];
    lib.onSignIn((acc) => seen.push(acc));
    await lib.connect();
    f.handlers["wallet:signIn"]!({ wallet: a, accounts: null });
    f.handlers["wallet:signIn"]!({ wallet: a, accounts: [{ accountId: null }] });
    expect(seen.map((s) => s.accountId)).toEqual([undefined, null]);
    expect(seen.every((s) => s.wallet === a)).toBe(true);
  });

  it("passes the library's sign-out event on", async () => {
    const f = fakeConnector(fakeWallet("A", ids("lamp.testnet")));
    const lib = nearConnectLibrary(f.connector);
    let outs = 0;
    lib.onSignOut(() => outs++);
    f.handlers["wallet:signOut"]!({});
    expect(outs).toBe(1);
  });
});
