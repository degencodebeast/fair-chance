import { describe, expect, it } from "vitest";
import { testnetRefusal, walletControl, type WalletAccount, type WalletLibrary, type WalletState } from "../src/wallet-session.ts";
import type { FinalOutcome, TxRequest } from "../src/tx-types.ts";

const IMPLICIT = "a".repeat(40) + "0123456789abcdef01234567";

describe("testnet account guard", () => {
  it("takes a named .testnet account on testnet", () => {
    expect(testnetRefusal({ accountId: "lamp.testnet", network: "testnet" })).toBeNull();
    expect(testnetRefusal({ accountId: "wick.lamp.testnet", network: "testnet" })).toBeNull();
    expect(testnetRefusal({ accountId: "big_fish-2.testnet", network: "testnet" })).toBeNull();
  });

  it("refuses a mainnet account name", () => {
    expect(testnetRefusal({ accountId: "lamp.near", network: "testnet" })).toBe(
      "This page works only with NEAR testnet accounts, so it signed out lamp.near.",
    );
  });

  it("refuses names that only look like testnet accounts", () => {
    for (const id of ["testnet", ".testnet", "lamp..testnet", "Lamp.testnet", "lamp.testnet.near", "lamp-.testnet", "lamp.testnet ", "0x" + "1".repeat(40), ""]) {
      expect(testnetRefusal({ accountId: id, network: "testnet" }), id).not.toBeNull();
    }
  });

  it("takes an implicit 64-hex account only when the library reports testnet", () => {
    expect(IMPLICIT).toHaveLength(64);
    expect(testnetRefusal({ accountId: IMPLICIT, network: "testnet" })).toBeNull();
    expect(testnetRefusal({ accountId: IMPLICIT.toUpperCase(), network: "testnet" })).not.toBeNull();
    expect(testnetRefusal({ accountId: IMPLICIT.slice(1), network: "testnet" })).not.toBeNull();
  });

  it("refuses any network the wallet reports other than testnet, even for a .testnet name", () => {
    expect(testnetRefusal({ accountId: "lamp.testnet", network: "mainnet" })).toBe(
      "This page works only on NEAR testnet, and the wallet is on mainnet, so it signed out.",
    );
    expect(testnetRefusal({ accountId: IMPLICIT, network: "mainnet" })).not.toBeNull();
    expect(testnetRefusal({ accountId: "lamp.testnet", network: "" })).not.toBeNull();
  });
});


const PAGE = "https://example.test/?network=testnet&view=coins";

/** A fake wallet library: it records every call and answers as the test sets. */
function fakeWallet(start: { connect?: WalletAccount | Error; restore?: WalletAccount | null; outcomes?: unknown[] } = {}) {
  const calls: string[] = [];
  const sent: Array<{ signerId: string; txs: TxRequest[] }> = [];
  let signIn: ((a: WalletAccount) => void) | null = null;
  let signOut: (() => void) | null = null;
  const lib: WalletLibrary = {
    async connect() {
      calls.push("connect");
      const r = start.connect ?? { accountId: "lamp.testnet", network: "testnet" };
      if (r instanceof Error) throw r;
      return r;
    },
    async restore() {
      calls.push("restore");
      return start.restore ?? null;
    },
    async send(signerId, txs) {
      calls.push("send");
      sent.push({ signerId, txs });
      return start.outcomes ?? txs.map((t, i) => outcome(signerId, t.receiverId, `hash${i}`));
    },
    async signOut() {
      calls.push("signOut");
    },
    onSignIn: (l) => void (signIn = l),
    onSignOut: (l) => void (signOut = l),
  };
  return { lib, calls, sent, emitSignIn: (a: WalletAccount) => signIn!(a), emitSignOut: () => signOut!() };
}

function outcome(signer: string, receiver: string, hash: string): FinalOutcome {
  return {
    transaction: { hash, signer_id: signer, receiver_id: receiver },
    transaction_outcome: { id: hash, block_hash: "b", outcome: { status: { SuccessValue: "" }, logs: [] } },
    receipts_outcome: [],
    status: { SuccessValue: "" },
  };
}

const BUY: TxRequest = { receiverId: "bag.factory.fairchance.testnet", actions: [{ type: "FunctionCall", params: { methodName: "buy", args: {}, gas: "1", deposit: "1" } }] };
const SELL: TxRequest = { receiverId: "bag.factory.fairchance.testnet", actions: [{ type: "FunctionCall", params: { methodName: "sell", args: {}, gas: "1", deposit: "1" } }] };

function signedIn(state: WalletState) {
  if (state.kind !== "signed-in") throw new Error(`expected signed in, got ${JSON.stringify(state)}`);
  return state.session;
}

describe("wallet control", () => {
  it("connects a testnet account and sends transactions in order, one outcome each", async () => {
    const w = fakeWallet();
    const control = walletControl(PAGE, () => w.lib);
    const session = signedIn(await control.connect());
    expect(session.accountId).toBe("lamp.testnet");
    const outcomes = await session.send([BUY, SELL]);
    expect(outcomes.map((o) => o.transaction.hash)).toEqual(["hash0", "hash1"]);
    expect(w.sent).toEqual([{ signerId: "lamp.testnet", txs: [BUY, SELL] }]);
  });

  it("signs out a mainnet account at once and shows one sentence", async () => {
    const w = fakeWallet({ connect: { accountId: "lamp.near", network: "mainnet" } });
    const control = walletControl(PAGE, () => w.lib);
    const state = await control.connect();
    expect(state).toEqual({ kind: "signed-out", notice: "This page works only on NEAR testnet, and the wallet is on mainnet, so it signed out." });
    expect(w.calls).toEqual(["connect", "signOut"]);
    expect(control.state()).toEqual(state);
  });

  it("signs out a mainnet name even when the library says testnet", async () => {
    const w = fakeWallet({ connect: { accountId: "lamp.near", network: "testnet" } });
    const state = await walletControl(PAGE, () => w.lib).connect();
    expect(state.kind).toBe("signed-out");
    expect(w.calls).toEqual(["connect", "signOut"]);
  });

  it("keeps the refusal when the wallet's own sign-out fails", async () => {
    const w = fakeWallet({ connect: { accountId: "lamp.near", network: "testnet" } });
    w.lib.signOut = async () => {
      throw new Error("wallet gone");
    };
    const state = await walletControl(PAGE, () => w.lib).connect();
    expect(state).toEqual({ kind: "signed-out", notice: "This page works only with NEAR testnet accounts, so it signed out lamp.near." });
  });

  it("never opens the wallet library on a page that is not on testnet", async () => {
    for (const href of ["https://example.test/?network=mainnet", "https://example.test/?rpc=http://localhost:3030&coin=a.b", "https://example.test/"]) {
      let made = 0;
      const control = walletControl(href, () => (made++, fakeWallet().lib));
      expect(await control.restore(), href).toEqual({ kind: "signed-out", notice: null });
      expect(await control.connect(), href).toEqual({ kind: "signed-out", notice: "Wallet sign-in works only on a NEAR testnet page." });
      expect(made, href).toBe(0);
    }
  });

  it("restores a saved testnet session on page load, and refuses a saved mainnet one", async () => {
    const good = fakeWallet({ restore: { accountId: "lamp.testnet", network: "testnet" } });
    expect(signedIn(await walletControl(PAGE, () => good.lib).restore()).accountId).toBe("lamp.testnet");
    const none = fakeWallet();
    expect(await walletControl(PAGE, () => none.lib).restore()).toEqual({ kind: "signed-out", notice: null });
    const bad = fakeWallet({ restore: { accountId: "lamp.near", network: "mainnet" } });
    expect((await walletControl(PAGE, () => bad.lib).restore()).kind).toBe("signed-out");
    expect(bad.calls).toEqual(["restore", "signOut"]);
  });

  it("tells subscribers about sign-in, sign-out and a cancelled connect", async () => {
    const w = fakeWallet();
    const control = walletControl(PAGE, () => w.lib);
    const seen: string[] = [];
    const stop = control.subscribe((s) => seen.push(s.kind === "signed-in" ? s.session.accountId : `out:${s.notice}`));
    await control.connect();
    await control.signOut();
    expect(w.calls).toEqual(["connect", "signOut"]);
    w.lib.connect = async () => {
      throw new Error("User rejected");
    };
    await control.connect();
    stop();
    await control.connect();
    expect(seen).toEqual(["lamp.testnet", "out:null", "out:null"]);
  });

  it("follows sign-in and sign-out that start in the wallet, through the same guard", async () => {
    const w = fakeWallet();
    const control = walletControl(PAGE, () => w.lib);
    await control.restore();
    w.emitSignIn({ accountId: "wick.lamp.testnet", network: "testnet" });
    expect(signedIn(control.state()).accountId).toBe("wick.lamp.testnet");
    w.emitSignOut();
    expect(control.state()).toEqual({ kind: "signed-out", notice: null });
    w.emitSignIn({ accountId: "lamp.near", network: "testnet" });
    await Promise.resolve();
    expect(control.state().kind).toBe("signed-out");
    expect(w.calls.at(-1)).toBe("signOut");
  });

  it("keeps the session it gave when the wallet also reports the same sign-in", async () => {
    const w = fakeWallet();
    const control = walletControl(PAGE, () => w.lib);
    const session = signedIn(await control.connect());
    w.emitSignIn({ accountId: "lamp.testnet", network: "testnet" });
    expect(signedIn(control.state())).toBe(session);
    await expect(session.send([BUY])).resolves.toHaveLength(1);
  });

  it("refuses to send from a session that is signed out", async () => {
    const w = fakeWallet();
    const control = walletControl(PAGE, () => w.lib);
    const session = signedIn(await control.connect());
    await session.signOut();
    expect(control.state().kind).toBe("signed-out");
    await expect(session.send([BUY])).rejects.toThrow("The wallet is signed out.");
    expect(w.calls).not.toContain("send");
  });

  it("refuses an answer that does not match the transactions sent", async () => {
    const cases: unknown[][] = [
      [outcome("lamp.testnet", BUY.receiverId, "h")],
      [outcome("other.testnet", BUY.receiverId, "h"), outcome("lamp.testnet", SELL.receiverId, "i")],
      [outcome("lamp.testnet", "elsewhere.testnet", "h"), outcome("lamp.testnet", SELL.receiverId, "i")],
      [{ status: {} }, outcome("lamp.testnet", SELL.receiverId, "i")],
    ];
    for (const outcomes of cases) {
      const session = signedIn(await walletControl(PAGE, () => fakeWallet({ outcomes }).lib).connect());
      await expect(session.send([BUY, SELL])).rejects.toThrow(/wallet/);
    }
    const session = signedIn(await walletControl(PAGE, () => fakeWallet().lib).connect());
    await expect(session.send([])).rejects.toThrow();
  });

  it("makes the wallet library once, however often the page connects", async () => {
    let made = 0;
    const w = fakeWallet();
    const control = walletControl(PAGE, () => (made++, w.lib));
    await control.restore();
    await control.connect();
    await control.signOut();
    await control.connect();
    expect(made).toBe(1);
  });
});
