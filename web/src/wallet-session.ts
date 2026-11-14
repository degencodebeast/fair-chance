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
  /** The wallet that gave the account, as the library knows it. The session sends and signs out through this wallet only. */
  wallet?: unknown;
}

// NEAR account rules: parts of lowercase letters and digits, joined by one "-" or "_", split by ".".
const PART = "[a-z0-9]+(?:[-_][a-z0-9]+)*";
const TESTNET_NAME = new RegExp(`^(?:${PART}\\.)+testnet$`);
const IMPLICIT = /^[0-9a-f]{64}$/;

const ACCOUNT_REFUSED = "This page works only with NEAR testnet accounts, so it signed out the account.";

/**
 * Null when the page may use the account; else the one sentence the visitor sees. The account comes
 * from the wallet, so every field is checked for its type before use, and this function never throws.
 */
export function testnetRefusal(account: WalletAccount): string | null {
  if (typeof account !== "object" || account === null) return ACCOUNT_REFUSED;
  const id: unknown = account.accountId;
  const network: unknown = account.network;
  if (network !== "testnet") {
    const named = typeof network === "string" && network !== "" ? network : "another network";
    return `This page works only on NEAR testnet, and the wallet is on ${named}, so it signed out.`;
  }
  if (typeof id !== "string") return ACCOUNT_REFUSED;
  if ((id.length <= 64 && TESTNET_NAME.test(id)) || IMPLICIT.test(id)) return null;
  return id === "" ? ACCOUNT_REFUSED : `This page works only with NEAR testnet accounts, so it signed out ${id}.`;
}

/** What the page needs from a wallet library. wallet-adapter.ts gives the real one; tests give a fake. */
export interface WalletLibrary {
  /** Opens the wallet picker and signs in. Rejects when the visitor closes it. */
  connect(): Promise<WalletAccount>;
  /** The account of a session saved by an earlier visit; null when there is none. */
  restore(): Promise<WalletAccount | null>;
  /** Asks the wallet that gave `account` to sign and send the transactions as that account. The visitor approves them in the wallet. */
  send(account: WalletAccount, txs: TxRequest[]): Promise<unknown[]>;
  /** Signs out the wallet that gave `account`; with no account, the wallet the library has selected. */
  signOut(account?: WalletAccount): Promise<void>;
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
  const seen = new Map<string, number>();
  return answer.map((item, i) => {
    const tx = (item as Partial<FinalOutcome> | null)?.transaction;
    if (typeof tx?.hash !== "string" || tx.hash === "") throw new Error(`The wallet result ${i + 1} has no transaction hash.`);
    const first = seen.get(tx.hash);
    if (first !== undefined) throw new Error(`The wallet result ${i + 1} repeats the transaction of result ${first + 1}.`);
    seen.set(tx.hash, i);
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
  /** The account, with its wallet, of the signed-in session; null when signed out. */
  let bound: WalletAccount | null = null;
  let library: WalletLibrary | null = null;

  // A listener that throws must not stop the guard or the other listeners.
  const set = (next: WalletState, account: WalletAccount | null = null): WalletState => {
    current = next;
    bound = next.kind === "signed-in" ? account : null;
    for (const l of listeners) {
      try {
        l(next);
      } catch {
        // The listener's own fault; the state is already set.
      }
    }
    return next;
  };

  const quietly = async (signOut: () => Promise<void>): Promise<void> => {
    try {
      await signOut();
    } catch {
      // The page is already signed out; a wallet that fails to sign out changes nothing here.
    }
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

  const sessionFor = (account: WalletAccount): WalletSession => {
    const session: WalletSession = {
      accountId: account.accountId,
      async send(txs) {
        if (current.kind !== "signed-in" || current.session !== session) throw new Error("The wallet is signed out.");
        if (txs.length === 0) throw new Error("There is no transaction to send.");
        return checkedOutcomes(account.accountId, txs, await lib().send(account, txs));
      },
      signOut: () => control.signOut(),
    };
    return session;
  };

  /** Every account passes the guard here. A refused one is signed out before the page can use it. */
  // It never rejects: any error signs the wallet out and shows a notice.
  const take = async (given: WalletAccount): Promise<WalletState> => {
    let refusal: string | null = ACCOUNT_REFUSED;
    let account = given;
    try {
      // One copy of the fields, so the wallet cannot change them after the guard reads them.
      account = { accountId: given.accountId, network: given.network, wallet: given.wallet };
      refusal = testnetRefusal(account);
    } catch {
      // A wallet answer that cannot be read is refused.
    }
    if (refusal === null) {
      // The library also reports the sign-in that connect() made; keep the session already given out.
      if (bound !== null && bound.accountId === account.accountId && bound.wallet === account.wallet) return current;
      return set({ kind: "signed-in", session: sessionFor(account) }, account);
    }
    set({ kind: "signed-out", notice: refusal });
    await quietly(() => lib().signOut(account));
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
      let account: WalletAccount | null;
      try {
        account = await lib().restore();
      } catch {
        return current;
      }
      return account === null ? current : take(account);
    },
    async signOut() {
      const was = bound ?? undefined;
      set(SIGNED_OUT);
      const lib = library;
      if (lib !== null) await quietly(() => lib.signOut(was));
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => void listeners.delete(listener);
    },
  };
  return control;
}
