// The wallet session for NEAR testnet. The wallet library sits behind a thin adapter
// (wallet-adapter.ts), so tests drive this module with a fake wallet. Every account the wallet
// gives passes one guard first: a named .testnet account, or a 64-hex implicit account when the
// wallet runs on testnet. Any other account, or any other network, is signed out at once.
import { isNetworkName } from "./known-deployments.ts";
import type { FinalOutcome, TxRequest, WalletSession } from "./tx-types.ts";

/** One account as the wallet library reports it, with the network the library runs on. */
export interface WalletAccount {
  accountId: string;
  network: string;
}

// NEAR account rules: parts of lowercase letters and digits, joined by one "-" or "_", split by ".".
const PART = "[a-z0-9]+(?:[-_][a-z0-9]+)*";
const TESTNET_NAME = new RegExp(`^(?:${PART}\\.)+testnet$`);
const IMPLICIT = /^[0-9a-f]{64}$/;

/** Null when the page may use the account; else the one sentence the visitor sees. */
export function testnetRefusal(account: WalletAccount): string | null {
  if (account.network !== "testnet") {
    const named = account.network === "" ? "another network" : account.network;
    return `This page works only on NEAR testnet, and the wallet is on ${named}, so it signed out.`;
  }
  const id = account.accountId;
  if ((id.length <= 64 && TESTNET_NAME.test(id)) || IMPLICIT.test(id)) return null;
  return `This page works only with NEAR testnet accounts, so it signed out ${id === "" ? "the account" : id}.`;
}

/** What the page needs from a wallet library. wallet-adapter.ts gives the real one; tests give a fake. */
export interface WalletLibrary {
  /** Opens the wallet picker and signs in. Rejects when the visitor closes it. */
  connect(): Promise<WalletAccount>;
  /** The account of a session saved by an earlier visit; null when there is none. */
  restore(): Promise<WalletAccount | null>;
  /** Asks the wallet to sign and send the transactions. The visitor approves them in the wallet. */
  send(signerId: string, txs: TxRequest[]): Promise<unknown[]>;
  signOut(): Promise<void>;
  /** A sign-in that starts in the wallet, not from the page's button. */
  onSignIn(listener: (account: WalletAccount) => void): void;
  /** A sign-out that starts in the wallet. */
  onSignOut(listener: () => void): void;
}

export type WalletState = { kind: "signed-out"; notice: string | null } | { kind: "signed-in"; session: WalletSession };

export interface WalletControl {
  state(): WalletState;
  /** Opens the wallet picker. */
  connect(): Promise<WalletState>;
  /** Takes up a session saved by an earlier visit. Call once on page load. */
  restore(): Promise<WalletState>;
  signOut(): Promise<void>;
  /** Calls the listener on each change of state. Returns the function that stops it. */
  subscribe(listener: (state: WalletState) => void): () => void;
}

export const NOT_TESTNET_PAGE = "Wallet sign-in works only on a NEAR testnet page.";
const SIGNED_OUT: WalletState = { kind: "signed-out", notice: null };

/** True only when the page link names the testnet from the fixed config. A local sandbox link is not. */
function isTestnetPage(href: string): boolean {
  const network = new URL(href).searchParams.get("network");
  return network === "testnet" && isNetworkName(network);
}

/** Checks the wallet's answer against what was sent: one outcome per transaction, in order, from this account. */
function checkedOutcomes(signerId: string, txs: TxRequest[], answer: unknown[]): FinalOutcome[] {
  if (!Array.isArray(answer) || answer.length !== txs.length) {
    throw new Error(`The wallet sent back ${Array.isArray(answer) ? answer.length : "no"} results for ${txs.length} transactions.`);
  }
  return answer.map((item, i) => {
    const tx = (item as Partial<FinalOutcome> | null)?.transaction;
    if (typeof tx?.hash !== "string" || tx.hash === "") throw new Error(`The wallet result ${i + 1} has no transaction hash.`);
    if (tx.signer_id !== signerId) throw new Error(`The wallet result ${i + 1} is signed by another account.`);
    if (tx.receiver_id !== txs[i]!.receiverId) throw new Error(`The wallet result ${i + 1} goes to another receiver.`);
    return item as FinalOutcome;
  });
}

/**
 * The page's one wallet control. `makeLibrary` runs once, on first use, and only when the page link
 * names the testnet, so a page on any other network never loads or calls the wallet library.
 */
export function walletControl(href: string, makeLibrary: () => WalletLibrary): WalletControl {
  const testnetPage = isTestnetPage(href);
  const listeners = new Set<(state: WalletState) => void>();
  let current: WalletState = SIGNED_OUT;
  let library: WalletLibrary | null = null;

  const set = (next: WalletState): WalletState => {
    current = next;
    for (const l of listeners) l(next);
    return next;
  };

  const lib = (): WalletLibrary => {
    if (library === null) {
      library = makeLibrary();
      library.onSignIn((account) => void take(account));
      library.onSignOut(() => {
        if (current.kind === "signed-in") set(SIGNED_OUT);
      });
    }
    return library;
  };

  const sessionFor = (accountId: string): WalletSession => {
    const session: WalletSession = {
      accountId,
      async send(txs) {
        if (current.kind !== "signed-in" || current.session !== session) throw new Error("The wallet is signed out.");
        if (txs.length === 0) throw new Error("There is no transaction to send.");
        return checkedOutcomes(accountId, txs, await lib().send(accountId, txs));
      },
      signOut: () => control.signOut(),
    };
    return session;
  };

  /** Every account passes the guard here. A refused one is signed out before the page can use it. */
  const take = async (account: WalletAccount): Promise<WalletState> => {
    const refusal = testnetRefusal(account);
    if (refusal === null) {
      // The library also reports the sign-in that connect() made; keep the session already given out.
      if (current.kind === "signed-in" && current.session.accountId === account.accountId) return current;
      return set({ kind: "signed-in", session: sessionFor(account.accountId) });
    }
    set({ kind: "signed-out", notice: refusal });
    await lib().signOut().catch(() => {});
    return current;
  };

  const control: WalletControl = {
    state: () => current,
    async connect() {
      if (!testnetPage) return set({ kind: "signed-out", notice: NOT_TESTNET_PAGE });
      let account: WalletAccount;
      try {
        account = await lib().connect();
      } catch {
        return set(SIGNED_OUT);
      }
      return take(account);
    },
    async restore() {
      if (!testnetPage) return current;
      const account = await lib().restore().catch(() => null);
      return account === null ? current : take(account);
    },
    async signOut() {
      set(SIGNED_OUT);
      if (library !== null) await library.signOut().catch(() => {});
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
  };
  return control;
}
