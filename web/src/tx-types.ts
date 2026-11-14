// The shape of a transaction the page asks the wallet to sign, and of what comes back. The wallet
// session sends these; the transaction builders make them; the outcome parser reads the result.

/** One function call action, in the form NEAR wallets take. Gas and deposit are decimal strings. */
export interface FunctionCallAction {
  type: "FunctionCall";
  params: { methodName: string; args: Record<string, unknown>; gas: string; deposit: string };
}

/** One transaction to one receiver. */
export interface TxRequest {
  receiverId: string;
  actions: FunctionCallAction[];
}

/** The final execution outcome JSON from NEAR RPC (EXPERIMENTAL_tx_status or the wallet's result). */
export type FinalOutcome = {
  transaction: { hash: string; signer_id: string; receiver_id: string };
  transaction_outcome: { id: string; block_hash: string; outcome: { status: unknown; logs: string[] } };
  receipts_outcome: Array<{ id: string; block_hash: string; outcome: { status: unknown; logs: string[]; executor_id: string } }>;
  status: unknown;
};

/** A connected testnet wallet. send() resolves with one outcome per transaction, in order. */
export interface WalletSession {
  accountId: string;
  send(txs: TxRequest[]): Promise<FinalOutcome[]>;
  signOut(): Promise<void>;
}
