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
  manifest: { features?: { testnet?: boolean } };
  getAccounts(data: { network: "testnet" }): Promise<Array<{ accountId: string }>>;
  signAndSendTransactions(params: { network: "testnet"; signerId: string; transactions: TxRequest[] }): Promise<unknown[]>;
}

/** The part of NearConnector the page uses. */
export interface ConnectorLike {
  network: string;
  connect(): Promise<WalletLike>;
  wallet(): Promise<WalletLike>;
  disconnect(): Promise<void>;
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

export function nearConnectLibrary(connector: ConnectorLike = new NearConnector(connectorOptions())): WalletLibrary {
  // The network the page reports is the library's, and only when the chosen wallet says it works there.
  const networkOf = (wallet: WalletLike): string =>
    connector.network === "testnet" && wallet.manifest.features?.testnet !== true ? "" : connector.network;
  const accountOf = async (wallet: WalletLike): Promise<WalletAccount> => {
    const [first] = await wallet.getAccounts({ network: "testnet" });
    if (first === undefined) throw new Error("The wallet gave no account.");
    return { accountId: first.accountId, network: networkOf(wallet) };
  };
  return {
    connect: async () => accountOf(await connector.connect()),
    async restore() {
      const wallet = await connector.wallet().catch(() => null);
      return wallet === null ? null : accountOf(wallet).catch(() => null);
    },
    send: async (signerId, transactions) => (await connector.wallet()).signAndSendTransactions({ network: "testnet", signerId, transactions }),
    signOut: () => connector.disconnect(),
    onSignIn(listener) {
      connector.on("wallet:signIn", ({ wallet, accounts }) => {
        const first = accounts[0];
        if (first !== undefined) listener({ accountId: first.accountId, network: networkOf(wallet) });
      });
    },
    onSignOut(listener) {
      connector.on("wallet:signOut", () => listener());
    },
  };
}
