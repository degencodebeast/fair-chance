// The pre-deploy privacy scan of the built page. It reads every file in the build folder and fails
// on anything that must not go public: a wallet that is not one of our known accounts, a workspace
// or home folder path, a key or token, a private endpoint URL, and the host name of a keyed RPC
// provider. A finding names the file and the rule, with a short hint that never holds the value.
//   node scripts/scan-build-output.ts [dist folder]    (default: dist, next to this scripts folder)
// The builder's keyed RPC host goes in FC_PRIVATE_HOSTS (comma separated host names), set in the
// shell for the run. The scan reads that variable only; it never reads a file for it.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { pathToFileURL } from "node:url";
import { KNOWN_NETWORKS, type KnownNetwork } from "../src/known-deployments.ts";

export interface ScanOptions {
  /** Accounts the page may name. A sub-account of one of them is allowed too. */
  allowedAccounts: string[];
  /** Made-up account names the page shows as examples. Exact names only, no sub-accounts. */
  sampleAccounts: string[];
  /** Host names a URL in the build may use. */
  allowedHosts: string[];
  /** Host names of keyed RPC providers, added to the built-in list. */
  providerHosts: string[];
}

export interface Finding {
  file: string;
  rule: "wallet" | "workspace-path" | "key" | "private-endpoint" | "provider-host";
  /** The first characters of the match and its length. Never the whole match. */
  hint: string;
}

// Host names of common keyed RPC providers. A suffix match: "near-mainnet.infura.io" holds "infura.io".
const BUILT_IN_PROVIDER_HOSTS = [
  "infura.io", "alchemy.com", "alchemyapi.io", "quiknode.pro", "quicknode.com", "ankr.com", "getblock.io", "drpc.org",
  "chainstack.com", "blastapi.io", "tatum.io", "nownodes.io", "dwellir.com", "lavanet.xyz", "lava.build", "pagoda.co",
  "pokt.network", "onfinality.io", "bware.dev", "rpc.web4.near.page",
];

// Each pattern starts with a bracket class, so the private-string scan of the repo never matches this file.
const WALLET = /(?<![A-Za-z0-9_.-])[a-z0-9][a-z0-9_-]*(?:\.[a-z0-9_-]+)*\.(?:testnet|near)(?![A-Za-z0-9_-])/g;
const IMPLICIT_ACCOUNT = /(?<![A-Fa-f0-9])[a-f0-9]{64}(?![A-Fa-f0-9])/g;
const WORKSPACE_PATHS = [
  /[/][U]sers[/][A-Za-z0-9._-]+/g,
  /[/][h]ome[/][A-Za-z0-9._-]+/g,
  /[A-Za-z]:\\+[U]sers\\+/g,
  /[/][p]rivate[/](?:var|tmp)/g,
  /[n]ear-hack/gi,
  /(?<![A-Za-z0-9_])[.][o]mc[/]/g,
  /[.][s]cratch[/]/g,
  /[p]laybooks[/]/g,
  /[.][c]onfig[/]fair-chance/g,
];
const KEYS = [
  /[e]d25519:[1-9A-HJ-NP-Za-km-z]{40,}/g,
  /[s]ecp256k1:[1-9A-HJ-NP-Za-km-z]{40,}/g,
  /[B]earer\s+[A-Za-z0-9._~+/=-]{20,}/g,
  /-----BEGIN [A-Z ]*[P]RIVATE KEY-----/g,
  /[s]k-[A-Za-z0-9]{32,}/g,
];
const URL_PATTERN = /[h]ttps?:\/\/[^\s"'`<>)\\]+/g;
const KEY_PARAMETER = /[?&;](?:api[_-]?key|apikey|key|token|access[_-]?token|auth)=[^&\s]+/i;

function hint(match: string): string {
  return `${match.slice(0, 3)}... (${match.length} characters)`;
}

function walk(root: string, folder = root): string[] {
  return readdirSync(folder).flatMap((name) => {
    const path = join(folder, name);
    return statSync(path).isDirectory() ? walk(root, path) : [path];
  });
}

function isAllowedAccount(account: string, allowed: string[]): boolean {
  return allowed.some((a) => account === a || account.endsWith(`.${a}`));
}

function hostMatches(host: string, names: string[]): boolean {
  return names.some((n) => host === n || host.endsWith(`.${n}`));
}

/** A minified script reads `t.testnet` for the property `testnet` of a variable. A quoted name is never that. */
function isScriptProperty(text: string, match: RegExpMatchArray, file: string): boolean {
  if (!/\.(?:js|mjs)$/.test(file)) return false;
  const before = text[match.index! - 1] ?? "";
  return !/["'`]/.test(before) && /^[a-z0-9_$]{1,2}\.(?:testnet|near)$/.test(match[0]);
}

function scanText(text: string, file: string, options: ScanOptions): Finding[] {
  const found: Finding[] = [];
  const add = (rule: Finding["rule"], match: string) => found.push({ file, rule, hint: hint(match) });
  for (const m of text.matchAll(WALLET)) {
    if (options.sampleAccounts.includes(m[0]) || isAllowedAccount(m[0], options.allowedAccounts)) continue;
    if (isScriptProperty(text, m, file)) continue;
    add("wallet", m[0]);
  }
  for (const m of text.matchAll(IMPLICIT_ACCOUNT)) add("wallet", m[0]);
  for (const pattern of WORKSPACE_PATHS) for (const m of text.matchAll(pattern)) add("workspace-path", m[0]);
  for (const pattern of KEYS) for (const m of text.matchAll(pattern)) add("key", m[0]);
  for (const m of text.matchAll(URL_PATTERN)) {
    let url: URL | null = null;
    try {
      url = new URL(m[0]);
    } catch {
      add("private-endpoint", m[0]);
      continue;
    }
    if (url.username !== "" || url.password !== "" || KEY_PARAMETER.test(m[0]) || !hostMatches(url.hostname, options.allowedHosts)) add("private-endpoint", m[0]);
  }
  const lower = text.toLowerCase();
  for (const host of [...BUILT_IN_PROVIDER_HOSTS, ...options.providerHosts]) {
    if (host === "") continue;
    let at = lower.indexOf(host.toLowerCase());
    while (at !== -1) {
      add("provider-host", host);
      at = lower.indexOf(host.toLowerCase(), at + host.length);
    }
  }
  return found;
}

/** Scans every file under `dir`. Throws when the folder is missing or empty, so a missing build never passes. */
export async function scanBuildOutput(dir: string, options: ScanOptions): Promise<Finding[]> {
  const files = walk(dir);
  if (files.length === 0) throw new Error("the build folder has no files");
  return files.flatMap((path) => scanText(readFileSync(path, "latin1"), relative(dir, path).split("\\").join("/"), options));
}

// The made-up names in the sample data and the input hints of the page.
const SAMPLE_ACCOUNTS = ["moth.factory.testnet", "you.testnet", "bigfish.testnet", "dart.testnet", "lamp.testnet", "wick.lamp.testnet"];

/** The accounts and hosts the page may name, from the fixed config. */
export function optionsFromConfig(env: Record<string, string | undefined> = process.env): ScanOptions {
  const networks: KnownNetwork[] = Object.values(KNOWN_NETWORKS);
  const hostOf = (url: string) => new URL(url).hostname;
  return {
    allowedAccounts: networks.flatMap((n) => [n.factory, n.demoCoin]).filter((a): a is string => a !== null),
    allowedHosts: [
      ...networks.flatMap((n) => [hostOf(n.rpcUrl), hostOf(n.fastNearUrl), hostOf(n.explorerTxUrl)]),
      "fonts.googleapis.com", "fonts.gstatic.com", "www.w3.org",
    ],
    sampleAccounts: SAMPLE_ACCOUNTS,
    providerHosts: (env.FC_PRIVATE_HOSTS ?? "").split(",").map((h) => h.trim()).filter((h) => h !== ""),
  };
}

if (import.meta.url === pathToFileURL(process.argv[1] ?? "").href) {
  const dir = process.argv[2] ?? new URL("../dist", import.meta.url).pathname;
  try {
    const findings = await scanBuildOutput(dir, optionsFromConfig());
    for (const f of findings) console.error(`scan-build-output: ${f.rule} in ${f.file}: ${f.hint}`);
    if (findings.length > 0) {
      console.error(`scan-build-output: FAIL ${findings.length} finding${findings.length === 1 ? "" : "s"}`);
      process.exit(1);
    }
    console.log(`scan-build-output: clean (${walk(dir).length} files scanned)`);
  } catch (err) {
    console.error(`scan-build-output: FAIL ${err instanceof Error ? err.message : String(err)}`);
    process.exit(1);
  }
}
