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
  /** Exact values the page may hold although they look like a key: the code hash and the accounts of the fixed config. */
  allowedValues: string[];
  /** Made-up account names the page shows as examples. Exact names only, no sub-accounts. */
  sampleAccounts: string[];
  /** The hosts a URL in the build may use, each with the one path shape it may have. Exact host names. */
  allowedHosts: Record<string, HostRule>;
  /** Host names of keyed RPC providers, added to the built-in list. */
  providerHosts: string[];
}

export interface HostRule {
  /** Matched against the whole path of the URL. */
  path: RegExp;
  /** The only query keys the URL may carry. No list means no query. */
  queryKeys?: string[];
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
const KEY_PARAMETER = /(?<![A-Za-z0-9])(?:x-api-key|api[_-]?key|apikey|access[_-]?token|auth[_-]?token|token|secret|password|passwd|key|auth|sig|signature)=[^&\s]+/i;
// A field or header whose name ends in key, token, secret or password, with a value of 16 or more characters.
const KEY_FIELD = /(?<![A-Za-z0-9])[A-Za-z0-9_-]*(?:key|token|secret|passwd|password)["']?\s*[:=]\s*["'`]?[A-Za-z0-9_\-./+=]{16,}/gi;
const KEYS = [
  /[e]d25519:[1-9A-HJ-NP-Za-km-z]{40,}/g,
  /[s]ecp256k1:[1-9A-HJ-NP-Za-km-z]{40,}/g,
  /[B]earer\s+[A-Za-z0-9._~+/=-]{20,}/g,
  /-----BEGIN [A-Z ]*[P]RIVATE KEY-----/g,
  /[s]k-[A-Za-z0-9]{32,}/g,
  /[e]yJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{4,}/g,
  /[g]h[pousr]_[A-Za-z0-9]{36,}/g,
  /[g]ithub_pat_[A-Za-z0-9_]{20,}/g,
  /[s]k_(?:live|test)_[A-Za-z0-9]{8,}/g,
  /(?<![A-Za-z0-9])(?:AKIA|ASIA)[0-9A-Z]{16}(?![A-Za-z0-9])/g,
  /(?<![A-Za-z0-9])[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}(?![A-Za-z0-9])/g,
  KEY_FIELD,
];
const HEX_KEY = /(?<![A-Za-z0-9])[0-9a-fA-F]{32,}(?![A-Za-z0-9])/g;

/** Shannon entropy in bits per character. */
function entropy(text: string): number {
  const counts = new Map<string, number>();
  for (const c of text) counts.set(c, (counts.get(c) ?? 0) + 1);
  let bits = 0;
  for (const n of counts.values()) bits -= (n / text.length) * Math.log2(n / text.length);
  return bits;
}

/** A long bare string with a spread of characters reads as a secret. */
function looksLikeSecret(token: string): boolean {
  const classes = [/[a-z]/, /[A-Z]/, /[0-9]/].filter((r) => r.test(token)).length;
  const bits = entropy(token);
  return (token.length >= 24 && classes === 3 && bits >= 4.2) || (token.length >= 32 && bits >= 4.0);
}

const URL_PATTERN = /(?:\b(?:https?|wss?|ftp):\/\/|(?<![A-Za-z0-9:/\\.])\/\/(?=[A-Za-z0-9-]{2,}))[^\s"'`<>)\\]+/gi;
const BARE_TOKEN = /(?<![A-Za-z0-9_+/=-])[A-Za-z0-9_+/=-]{24,}(?![A-Za-z0-9_+/=-])/g;
const ETH_ACCOUNT = /(?<![A-Za-z0-9])0x[a-fA-F0-9]{40}(?![A-Za-z0-9])/g;

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

/** A URL is allowed when its host is listed and its scheme, path, query and login all fit that host's rule. */
function isAllowedUrl(text: string, hosts: Record<string, HostRule>): boolean {
  let url: URL;
  try {
    url = new URL(text.startsWith("//") ? `https:${text}` : text);
  } catch {
    return false;
  }
  const rule = Object.hasOwn(hosts, url.hostname) ? hosts[url.hostname] : undefined;
  if (rule === undefined || (url.protocol !== "https:" && url.protocol !== "http:")) return false;
  if (url.username !== "" || url.password !== "" || KEY_PARAMETER.test(text)) return false;
  if (url.port !== "" || url.hash !== "") return false;
  if (url.search !== "" && [...url.searchParams.keys()].some((k) => !(rule.queryKeys ?? []).includes(k))) return false;
  return rule.path.test(url.pathname);
}

/** The text with the escapes that hide a URL ("https:\\/\\/host") undone. */
function unescaped(text: string): string {
  return text.replace(/\\\//g, "/").replace(/\\u002[fF]/g, "/");
}

/**
 * For each character of a script: 1 when it is inside a string, a template text or a comment.
 * A name there is text the visitor can read, never code. The lexer does not know regex literals;
 * a quote inside one can shift the mask until the next line ends a string.
 */
function textMask(text: string): Uint8Array {
  const mask = new Uint8Array(text.length);
  const stack: { kind: "code" | "tpl"; braces: number; expr: boolean }[] = [{ kind: "code", braces: 0, expr: false }];
  let i = 0;
  while (i < text.length) {
    const top = stack[stack.length - 1]!;
    const c = text[i]!;
    if (top.kind === "tpl") {
      mask[i] = 1;
      if (c === "\\") {
        mask[i + 1] = 1;
        i += 2;
      } else if (c === "`") {
        stack.pop();
        i++;
      } else if (c === "$" && text[i + 1] === "{") {
        mask[i + 1] = 1;
        stack.push({ kind: "code", braces: 0, expr: true });
        i += 2;
      } else i++;
    } else if (c === '"' || c === "'") {
      mask[i] = 1;
      i++;
      while (i < text.length && text[i] !== c && text[i] !== "\n") {
        mask[i] = 1;
        i += text[i] === "\\" ? 2 : 1;
      }
      if (i < text.length) mask[i] = 1;
      i++;
    } else if (c === "/" && (text[i + 1] === "/" || text[i + 1] === "*")) {
      const end = text[i + 1] === "/" ? text.indexOf("\n", i) : text.indexOf("*/", i + 2);
      const stop = end === -1 ? text.length : end + (text[i + 1] === "*" ? 2 : 0);
      mask.fill(1, i, stop);
      i = Math.max(stop, i + 1);
    } else if (c === "`") {
      mask[i] = 1;
      stack.push({ kind: "tpl", braces: 0, expr: false });
      i++;
    } else {
      if (c === "{") top.braces++;
      else if (c === "}") {
        if (top.braces > 0) top.braces--;
        else if (top.expr) {
          stack.pop();
          mask[i] = 1;
        }
      }
      i++;
    }
  }
  return mask;
}

function scanText(text: string, file: string, options: ScanOptions, buildNames: ReadonlySet<string>): Finding[] {
  const found: Finding[] = [];
  const add = (rule: Finding["rule"], match: string) => found.push({ file, rule, hint: hint(match) });
  // One or two characters before ".testnet" in code (not in a string or a comment) is a property read of a minified script.
  const mask = /\.(?:js|mjs)$/.test(file) ? textMask(text) : null;
  for (const m of text.matchAll(WALLET)) {
    if (options.sampleAccounts.includes(m[0]) || isAllowedAccount(m[0], options.allowedAccounts)) continue;
    if (mask !== null && mask[m.index!] === 0 && /^[a-z0-9_$]{1,2}\.(?:testnet|near)$/.test(m[0])) continue;
    add("wallet", m[0]);
  }
  for (const m of text.matchAll(IMPLICIT_ACCOUNT)) add("wallet", m[0]);
  for (const pattern of WORKSPACE_PATHS) for (const m of text.matchAll(pattern)) add("workspace-path", m[0]);
  for (const pattern of KEYS) for (const m of text.matchAll(pattern)) add("key", m[0]);
  for (const m of text.matchAll(URL_PATTERN)) if (!isAllowedUrl(m[0], options.allowedHosts)) add("private-endpoint", m[0]);
  const known = (value: string) => options.allowedValues.includes(value) || buildNames.has(value);
  for (const m of text.matchAll(HEX_KEY)) if (m[0].length !== 64 && !known(m[0]) && entropy(m[0]) >= 3.0) add("key", m[0]);
  for (const m of text.matchAll(BARE_TOKEN)) if (!known(m[0]) && looksLikeSecret(m[0])) add("key", m[0]);
  for (const m of text.matchAll(ETH_ACCOUNT)) add("wallet", m[0]);
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
  const names = files.map((path) => relative(dir, path).split("\\").join("/"));
  // The exact names of the files of this build, as a script writes them: with or without the folder, the dot and the extension.
  const buildNames = new Set(
    names.flatMap((name) => {
      const noExtension = name.replace(/\.[A-Za-z0-9]+$/, "");
      const base = noExtension.slice(noExtension.lastIndexOf("/") + 1);
      return [name, noExtension, base, `/${base}`, `/${noExtension}`];
    }),
  );
  return files.flatMap((path, i) => {
    const name = names[i]!;
    // A file name is text too. When the name itself is a finding, every finding of the file shows the name masked.
    const inName = scanText(unescaped(name), "", options, buildNames);
    const shown = inName.length > 0 ? hint(name) : name;
    return [...inName, ...scanText(unescaped(readFileSync(path, "latin1")), name, options, buildNames)].map((f) => ({ ...f, file: shown }));
  });
}

// The made-up names in the sample data and the input hints of the page.
const SAMPLE_ACCOUNTS = ["moth.factory.testnet", "you.testnet", "bigfish.testnet", "dart.testnet", "lamp.testnet", "wick.lamp.testnet"];

/** The accounts and hosts the page may name, from the fixed config. */
export function optionsFromConfig(env: Record<string, string | undefined> = process.env): ScanOptions {
  const networks: KnownNetwork[] = Object.values(KNOWN_NETWORKS);
  const root: HostRule = { path: /^\/?$/ };
  const allowedHosts: Record<string, HostRule> = {
    "fonts.googleapis.com": { path: /^\/(?:css2)?$/, queryKeys: ["family", "display"] },
    "fonts.gstatic.com": root,
    "www.w3.org": { path: /^\/2000\/svg$/ },
  };
  for (const n of networks) {
    allowedHosts[new URL(n.rpcUrl).hostname] = root;
    allowedHosts[new URL(n.fastNearUrl).hostname] = root;
    const explorer = new URL(n.explorerTxUrl);
    allowedHosts[explorer.hostname] = { path: new RegExp(`^${explorer.pathname.replace(/\/$/, "").replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/?$`) };
  }
  return {
    allowedAccounts: networks.flatMap((n) => [n.factory, n.demoCoin]).filter((a): a is string => a !== null),
    allowedValues: networks.flatMap((n) => [n.factory, n.demoCoin, n.codeHash]).filter((v): v is string => v !== null),
    sampleAccounts: SAMPLE_ACCOUNTS,
    allowedHosts,
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
