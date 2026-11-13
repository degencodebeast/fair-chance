// The deployments this page trusts, fixed in the page code. The coin account checks compare a coin
// with these values. A page link never supplies them, except for a local sandbox, where each
// verify run deploys a new factory and coin hash.
export interface KnownDeployment {
  /** The factory account the coin must be a direct sub-account of; null until the deploy exists. */
  factory: string | null;
  /** The one coin code hash the factory records; null until the deploy exists. */
  codeHash: string | null;
}

/** Fill these two values after the testnet deploy. Until then the checks show "not checked". */
export const KNOWN_TESTNET: KnownDeployment = { factory: null, codeHash: null };

const NONE: KnownDeployment = { factory: null, codeHash: null };

function isLocalSandbox(rpcUrl: string): boolean {
  const host = new URL(rpcUrl).hostname;
  return host === "localhost" || host === "127.0.0.1";
}

/** The deployment to check a coin against, for the page link (its RPC URL and, on a local sandbox, its anchors). */
export function knownDeploymentFor(href: string): KnownDeployment {
  const params = new URL(href).searchParams;
  const rpcUrl = params.get("rpc");
  if (!rpcUrl) return NONE;
  if (isLocalSandbox(rpcUrl)) return { factory: params.get("factory"), codeHash: params.get("code_hash") };
  return new URL(rpcUrl).hostname.split(".").includes("testnet") ? KNOWN_TESTNET : NONE;
}
