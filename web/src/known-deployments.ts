// The networks this page trusts, fixed in the page code. A page link picks only the network by
// name. It never supplies an RPC URL, a factory or a coin code hash, except for a local sandbox
// (see page-link.ts), where each verify run deploys a new factory and coin hash.
export interface KnownDeployment {
  /** The factory account the coin must be a direct sub-account of; null until the deploy exists. */
  factory: string | null;
  /** The one coin code hash the factory records; null until the deploy exists. */
  codeHash: string | null;
}

export interface KnownNetwork extends KnownDeployment {
  /** The RPC URL the page reads through. */
  rpcUrl: string;
  /** The FastNEAR transactions API the page reads events through. */
  fastNearUrl: string;
}

/**
 * Fill `factory` and `codeHash` after the testnet deploy. Until then the checks show "not checked".
 * The RPC URL is the public FastNEAR testnet endpoint from the NEAR RPC providers list.
 */
export const KNOWN_NETWORKS = {
  testnet: {
    rpcUrl: "https://test.rpc.fastnear.com",
    fastNearUrl: "https://tx.test.fastnear.com",
    factory: null,
    codeHash: null,
  },
} as const satisfies Record<string, KnownNetwork>;

export type NetworkName = keyof typeof KNOWN_NETWORKS;

export function isNetworkName(name: string): name is NetworkName {
  return Object.hasOwn(KNOWN_NETWORKS, name);
}
