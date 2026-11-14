// The one file that knows the wallet library: @hot-labs/near-connect. Its wallets open in a pop-up,
// an extension or a sandboxed frame, so the page keeps its state. The library runs on testnet with
// the public RPC from the fixed config, never a keyed one. It only reports what the wallet says;
// the testnet guard in wallet-session.ts decides what the page may use.
import { NearConnector, type NearConnectorOptions } from "@hot-labs/near-connect";
import { KNOWN_NETWORKS } from "./known-deployments.ts";
import type { TxRequest } from "./tx-types.ts";
import type { WalletAccount, WalletLibrary } from "./wallet-session.ts";

/** The part of a near-connect wallet the page uses. */
export interface WalletLike {
  manifest: { id?: string; features?: { testnet?: boolean } };
  getAccounts(data: { network: "testnet" }): Promise<Array<{ accountId: string }>>;
  signAndSendTransactions(params: { network: "testnet"; signerId: string; transactions: TxRequest[] }): Promise<unknown[]>;
  signOut(data: { network: "testnet" }): Promise<void>;
}

/** The part of NearConnector the page uses. */
export interface ConnectorLike {
  network: string;
  connect(): Promise<WalletLike>;
  /** The wallet the library saved as selected. */
  wallet(): Promise<WalletLike>;
  /** Signs out the wallet and clears the saved selection. */
  disconnect(wallet?: WalletLike): Promise<void>;
  on(event: "wallet:signIn", cb: (payload: { wallet: WalletLike; accounts: Array<{ accountId: string }> }) => void): void;
  on(event: "wallet:signOut", cb: (payload: unknown) => void): void;
}

export function connectorOptions(): NearConnectorOptions {
  return {
    network: "testnet",
    providers: { testnet: [KNOWN_NETWORKS.testnet.rpcUrl] },
    // Show only wallets that say they work on testnet and can send a list of transactions.
    features: { testnet: true, signAndSendTransactions: true },
    // Nothing signs in without the visitor's click.
    autoConnect: false,
    footerBranding: null,
  };
}

/** The first account of a wallet answer. Anything else gives no account id, which the guard refuses. */
function firstAccountId(accounts: unknown): unknown {
  const first: unknown = Array.isArray(accounts) ? accounts[0] : undefined;
  return typeof first === "object" && first !== null ? (first as { accountId?: unknown }).accountId : undefined;
}

export function nearConnectLibrary(connector: ConnectorLike = new NearConnector(connectorOptions())): WalletLibrary {
  /** The wallet this page connected or restored. Only it may report a sign-in, send or be disconnected. */
  let selected: WalletLike | null = null;
  /** The library also emits a sign-in for the page's own connect; connect() reports that one itself. */
  let connecting = false;

  // near-connect does not check a wallet's network; the account guard and the outcome checks are the protection.
  const networkOf = (wallet: WalletLike): string =>
    connector.network === "testnet" && wallet.manifest?.features?.testnet !== true ? "" : connector.network;
  // It never throws once the wallet is known, so a bad answer still reaches the guard, which signs that wallet out.
  const accountOf = async (wallet: WalletLike): Promise<WalletAccount> => {
    let accounts: unknown;
    try {
      accounts = await wallet.getAccounts({ network: "testnet" });
    } catch {
      accounts = undefined;
    }
    return { accountId: firstAccountId(accounts) as string, network: networkOf(wallet), wallet };
  };
  const signOutOnly = async (wallet: WalletLike): Promise<void> => {
    try {
      await wallet.signOut({ network: "testnet" });
    } catch {
      // The page never takes this wallet's accounts, signed out or not.
    }
  };

  return {
    async connect() {
      connecting = true;
      let wallet: WalletLike;
      try {
        wallet = await connector.connect();
      } finally {
        connecting = false;
      }
      selected = wallet;
      return accountOf(wallet);
    },
    async restore() {
      const wallet = await connector.wallet().catch(() => null);
      if (wallet === null) return null;
      selected = wallet;
      return accountOf(wallet);
    },
    async send(account, transactions) {
      const wallet = account.wallet as WalletLike | undefined;
      if (wallet === undefined || typeof wallet.signAndSendTransactions !== "function") throw new Error("The wallet of this account is not known.");
      return wallet.signAndSendTransactions({ network: "testnet", signerId: account.accountId, transactions });
    },
    async signOut(account) {
      const wallet = account?.wallet as WalletLike | undefined;
      // A wallet that is not the selected one is signed out alone, so the selection stays.
      if (wallet !== undefined && wallet !== selected) return signOutOnly(wallet);
      selected = null;
      await connector.disconnect(wallet);
    },
    onSignIn(listener) {
      connector.on("wallet:signIn", (payload) => {
        if (connecting) return;
        const wallet = payload?.wallet;
        if (wallet === undefined || wallet === null) return;
        if (wallet !== selected) return void signOutOnly(wallet);
        listener({ accountId: firstAccountId(payload.accounts) as string, network: networkOf(wallet), wallet });
      });
    },
    onSignOut(listener) {
      connector.on("wallet:signOut", () => listener());
    },
  };
}
