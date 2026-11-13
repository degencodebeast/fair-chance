// The page checks the chain facts that make a coin a Fair Chance coin (builder answer, Q5):
// the coin is a direct sub-account of the known factory, its global contract hash equals the known
// coin hash, and it has no access keys. A failed or missing check shows a clear warning.
import type { AccountFacts, ChainReader } from "./chain.ts";

export interface KnownDeployment {
  /** The factory account the coin must be a direct sub-account of; null when the page link gives none. */
  factory: string | null;
  /** The one coin code hash the factory records; null when the page link gives none. */
  codeHash: string | null;
}

export type CheckStatus = "pass" | "fail" | "unchecked";

export interface CheckResult {
  id: "parent-account" | "code-hash" | "no-access-keys";
  label: string;
  status: CheckStatus;
  detail: string;
}

/** `?factory=<account>&code_hash=<hash>` in the page link. */
export function knownDeploymentFromUrl(href: string): KnownDeployment {
  const params = new URL(href).searchParams;
  return { factory: params.get("factory"), codeHash: params.get("code_hash") };
}

export function parentAccountCheck(coin: string, known: KnownDeployment): CheckResult {
  const label = "Coin is a direct sub-account of the factory";
  if (!known.factory) {
    return { id: "parent-account", label, status: "unchecked", detail: "This page has no known factory to compare with." };
  }
  const suffix = `.${known.factory}`;
  const direct = coin.endsWith(suffix) && coin.length > suffix.length && !coin.slice(0, -suffix.length).includes(".");
  return direct
    ? { id: "parent-account", label, status: "pass", detail: `${coin} sits directly under ${known.factory}.` }
    : { id: "parent-account", label, status: "fail", detail: `${coin} is not a direct sub-account of ${known.factory}.` };
}

export function codeHashCheck(account: AccountFacts, known: KnownDeployment): CheckResult {
  const label = "Coin runs the known code by hash";
  if (account.localCodeHash !== null) {
    return { id: "code-hash", label, status: "fail", detail: "The account holds code of its own, not a global contract by hash." };
  }
  if (account.usesGlobalContractByAccount) {
    return { id: "code-hash", label, status: "fail", detail: "The account uses a global contract by account name, so the owner of that account can change its code." };
  }
  if (account.globalContractHash === null) {
    return { id: "code-hash", label, status: "fail", detail: "The account has no global contract." };
  }
  if (!known.codeHash) {
    return { id: "code-hash", label, status: "unchecked", detail: `The account runs hash ${account.globalContractHash}. This page has no known hash to compare with.` };
  }
  return account.globalContractHash === known.codeHash
    ? { id: "code-hash", label, status: "pass", detail: `The account runs the known hash ${known.codeHash}.` }
    : { id: "code-hash", label, status: "fail", detail: `The account runs hash ${account.globalContractHash}, not the known hash ${known.codeHash}.` };
}

export function accessKeysCheck(keyCount: number): CheckResult {
  const label = "Coin has no access keys";
  return keyCount === 0
    ? { id: "no-access-keys", label, status: "pass", detail: "The account has no access keys, so nobody can sign for it." }
    : { id: "no-access-keys", label, status: "fail", detail: `The account has ${keyCount} access ${keyCount === 1 ? "key" : "keys"}.` };
}

export async function runCoinChecks(reader: ChainReader, coin: string, known: KnownDeployment): Promise<CheckResult[]> {
  const [account, keyCount] = await Promise.all([reader.account(coin), reader.accessKeyCount(coin)]);
  return [parentAccountCheck(coin, known), codeHashCheck(account, known), accessKeysCheck(keyCount)];
}
