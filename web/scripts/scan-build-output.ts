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
import { BOT_URL } from "../src/telegram-button.ts";
import { FAUCET_URL } from "../src/try-it-copy.ts";

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
  /** Whole-name patterns for the test accounts of our own testnet run. Each must match the full account. */
  ownTestAccounts?: RegExp[];
  /** Transaction hashes of the committed evidence files. Only the tx_hash fields of the top-level recorded-run.json may hold one. */
  evidenceTxHashes?: ReadonlySet<string>;
  /** Exact values that one build file may hold, such as the wallet library's own links. No other file gets them. */
  chunkAllowances?: ChunkAllowance[];
}

/** One reviewed value and why the page may hold it. */
export interface AllowedValue {
  value: string;
  reason: string;
}

export interface ChunkAllowance {
  /** Matched against the whole build name of the file, such as "assets/wallet-adapter-AbCd1234.js". */
  file: RegExp;
  /** Exact URLs. Nothing inside one of them is a finding: a host that reads like an account, a commit hash. */
  urls: AllowedValue[];
  /** Exact whole matches of a rule, such as an object name in library code that reads like an account. */
  values: AllowedValue[];
}

export interface HostRule {
  /** Matched against the whole path of the URL. */
  path: RegExp;
  /** The only query keys the URL may carry. No list means no query. */
  queryKeys?: string[];
}

export interface Finding {
  file: string;
  rule: "wallet" | "workspace-path" | "key" | "private-endpoint" | "provider-host" | "unreadable-file";
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

const REGEX_KEYWORDS = new Set(["return", "typeof", "instanceof", "in", "of", "case", "void", "new", "delete", "throw", "else", "do", "yield", "await"]);

/** True when a "/" at `at` starts a regex literal, from the last significant token before it. */
function startsRegex(text: string, at: number): boolean {
  let i = at - 1;
  while (i >= 0 && /\s/.test(text[i]!)) i--;
  if (i < 0) return true;
  const p = text[i]!;
  if (/[(,=:[!&|?{};+\-*%<>~^}]/.test(p)) return true;
  if (!/[A-Za-z0-9_$]/.test(p)) return false;
  let start = i;
  while (start > 0 && /[A-Za-z0-9_$]/.test(text[start - 1]!)) start--;
  return REGEX_KEYWORDS.has(text.slice(start, i + 1));
}

/** The index after the regex literal that starts at `at`, or -1 when it does not close on its line. */
function regexEnd(text: string, at: number): number {
  let i = at + 1;
  let inClass = false;
  while (i < text.length && text[i] !== "\n") {
    const c = text[i]!;
    if (c === "\\") i += 2;
    else if (c === "[") (inClass = true, i++);
    else if (c === "]") (inClass = false, i++);
    else if (c === "/" && !inClass) {
      i++;
      while (i < text.length && /[A-Za-z]/.test(text[i]!)) i++;
      return i;
    } else i++;
  }
  return -1;
}

/**
 * For each character of a script: 1 when it is inside a string, a template text, a regex or a
 * comment. A name there is text the visitor can read, never code. `ok` is false when the lexer
 * cannot close a string, a regex, a comment or a template: the file is then not passed.
 */
function textMask(text: string): { mask: Uint8Array; ok: boolean } {
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
      if (text[i] !== c) return { mask, ok: false };
      mask[i] = 1;
      i++;
    } else if (c === "/" && (text[i + 1] === "/" || text[i + 1] === "*")) {
      const end = text[i + 1] === "/" ? text.indexOf("\n", i) : text.indexOf("*/", i + 2);
      if (end === -1 && text[i + 1] === "*") return { mask, ok: false };
      const stop = end === -1 ? text.length : end + (text[i + 1] === "*" ? 2 : 0);
      mask.fill(1, i, stop);
      i = Math.max(stop, i + 1);
    } else if (c === "/" && startsRegex(text, i)) {
      const end = regexEnd(text, i);
      if (end === -1) return { mask, ok: false };
      mask.fill(1, i, end);
      i = end;
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
  return { mask, ok: stack.length === 1 };
}

function scanText(text: string, file: string, options: ScanOptions, buildNames: ReadonlySet<string>): Finding[] {
  const found: Finding[] = [];
  // The reviewed values of this one file. A value matches only as a whole, and only in this file.
  const chunk = (options.chunkAllowances ?? []).filter((a) => a.file.test(file));
  const chunkUrls = new Set(chunk.flatMap((a) => a.urls.map((u) => u.value)));
  const chunkValues = new Set(chunk.flatMap((a) => a.values.map((v) => v.value)));
  const chunkUrlRanges = [...text.matchAll(URL_PATTERN)].filter((u) => chunkUrls.has(u[0])).map((u) => [u.index!, u.index! + u[0].length] as const);
  const add = (rule: Finding["rule"], match: string, at?: number) => {
    if (chunkValues.has(match)) return;
    if (at !== undefined && chunkUrlRanges.some(([from, to]) => at >= from && at < to)) return;
    found.push({ file, rule, hint: hint(match) });
  };
  // One or two characters before ".testnet" in code (not in a string or a comment) is a property read of a minified script.
  const lexed = /\.(?:js|mjs)$/.test(file) ? textMask(text) : null;
  if (lexed !== null && !lexed.ok) add("unreadable-file", file);
  const mask = lexed?.ok ? lexed.mask : null;
  // A host name such as "docs.near.org" reads like an account name. Inside a URL the page may name, it is a host.
  const allowedUrlRanges = [...text.matchAll(URL_PATTERN)].filter((u) => isAllowedUrl(u[0], options.allowedHosts)).map((u) => [u.index!, u.index! + u[0].length] as const);
  for (const m of text.matchAll(WALLET)) {
    if (allowedUrlRanges.some(([from, to]) => m.index! >= from && m.index! < to)) continue;
    if (options.sampleAccounts.includes(m[0]) || isAllowedAccount(m[0], options.allowedAccounts)) continue;
    if (options.ownTestAccounts?.some((p) => p.test(m[0]))) continue;
    if (mask !== null && mask[m.index!] === 0 && /^[a-z0-9_$]{1,2}\.(?:testnet|near)$/.test(m[0])) continue;
    add("wallet", m[0], m.index);
  }
  for (const m of text.matchAll(IMPLICIT_ACCOUNT)) add("wallet", m[0], m.index);
  for (const pattern of WORKSPACE_PATHS) for (const m of text.matchAll(pattern)) add("workspace-path", m[0], m.index);
  for (const pattern of KEYS) for (const m of text.matchAll(pattern)) add("key", m[0], m.index);
  for (const m of text.matchAll(URL_PATTERN)) if (!isAllowedUrl(m[0], options.allowedHosts)) add("private-endpoint", m[0], m.index);
  const known = (value: string) => options.allowedValues.includes(value) || buildNames.has(value);
  for (const m of text.matchAll(HEX_KEY)) if (m[0].length !== 64 && !known(m[0]) && entropy(m[0]) >= 3.0) add("key", m[0], m.index);
  for (const m of text.matchAll(BARE_TOKEN)) if (!known(m[0]) && looksLikeSecret(m[0])) add("key", m[0], m.index);
  for (const m of text.matchAll(ETH_ACCOUNT)) add("wallet", m[0], m.index);
  const lower = text.toLowerCase();
  for (const host of [...BUILT_IN_PROVIDER_HOSTS, ...options.providerHosts]) {
    if (host === "") continue;
    let at = lower.indexOf(host.toLowerCase());
    while (at !== -1) {
      add("provider-host", host, at);
      at = lower.indexOf(host.toLowerCase(), at + host.length);
    }
  }
  return found;
}

const PUBLIC_FILES = ["index.html"];
const BUILT_ASSET = /^assets\/[a-z][a-z0-9]*(?:-[a-z0-9]+)*-[A-Za-z0-9_-]{8}\.(?:js|css|map)$/;

function isBuildName(name: string): boolean {
  return PUBLIC_FILES.includes(name) || BUILT_ASSET.test(name);
}

const RECORDED_RUN_FILE = "recorded-run.json";
const TX_HASH_FIELD = /"tx_hash"\s*:\s*"([1-9A-HJ-NP-Za-km-z]{43,44})"/g;

/**
 * The recorded run file holds public transaction hashes, which look like keys. A `"tx_hash": "<value>"` field
 * passes only when the value is a transaction hash of a committed evidence file. The scan reads the file text
 * with exactly those field values removed, so the same value anywhere else in the file is still a finding, and
 * so is any value that is not in the evidence. A file that is not valid JSON passes nothing. Only the file at the top of the build gets this.
 */
function withoutEvidenceTxHashes(text: string, evidence: ReadonlySet<string>): string {
  try {
    JSON.parse(text);
  } catch {
    return text;
  }
  return text.replace(TX_HASH_FIELD, (field, hash: string) => (evidence.has(hash) ? '"tx_hash":""' : field));
}

const EVIDENCE_FOLDER = new URL("../../evidence/", import.meta.url);

/** The values of the `tx_hash` and `tx` fields of the committed evidence files (evidence/*.json at the repo root). */
export function evidenceTxHashes(folder: URL = EVIDENCE_FOLDER): Set<string> {
  const found = new Set<string>();
  const visit = (node: unknown): void => {
    if (Array.isArray(node)) node.forEach(visit);
    else if (typeof node === "object" && node !== null) {
      for (const [key, value] of Object.entries(node)) {
        if ((key === "tx_hash" || key === "tx") && typeof value === "string") found.add(value);
        else visit(value);
      }
    }
  };
  let names: string[] = [];
  try {
    names = readdirSync(folder).filter((n) => n.endsWith(".json"));
  } catch {
    return found;
  }
  for (const name of names) visit(JSON.parse(readFileSync(new URL(name, folder), "utf8")));
  return found;
}

function fileText(path: string, name: string, options: ScanOptions): string {
  const text = readFileSync(path, "latin1");
  return name === RECORDED_RUN_FILE && options.evidenceTxHashes ? withoutEvidenceTxHashes(text, options.evidenceTxHashes) : text;
}

/** Scans every file under `dir`. Throws when the folder is missing or empty, so a missing build never passes. */
export async function scanBuildOutput(dir: string, options: ScanOptions): Promise<Finding[]> {
  const files = walk(dir);
  if (files.length === 0) throw new Error("the build folder has no files");
  const names = files.map((path) => relative(dir, path).split("\\").join("/"));
  // The names a build emits: index.html, and assets/<words>-<8-character hash>.<js|css|map> (the Vite hash is 8 characters
  // of letters, digits, "_" and "-"). Script imports name these files, so only these exact names are not read as keys.
  // Any other file name is scanned like text.
  const buildNames = new Set(
    names.filter(isBuildName).flatMap((name) => {
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
    return [...inName, ...scanText(unescaped(fileText(path, name, options)), name, options, buildNames)].map((f) => ({ ...f, file: shown }));
  });
}

// The made-up names in the sample data and the input hints of the page.
const SAMPLE_ACCOUNTS = ["moth.factory.testnet", "you.testnet", "bigfish.testnet", "dart.testnet", "lamp.testnet", "wick.lamp.testnet"];

// The test accounts of our own testnet run, which the recorded run file names. Exact shapes under
// the one root we own, so no other account and no sub-account of these passes.
const OWN_TEST_ACCOUNTS = [/^(?:bag-buyer-\d{2}|bag-creator|cabal-[a-z0-9-]+)\.fairchance\.testnet$/];

/**
 * The reviewed values of the wallet library chunk: @hot-labs/near-connect 0.12.0 with the pinned wallet
 * list of src/wallet-manifest.ts. Only that chunk gets them; the same value in any other file is a finding.
 * A new library version or a new wallet changes this list, and the change needs a review.
 */
export const WALLET_CHUNK: ChunkAllowance = {
  file: /^assets\/wallet-adapter-[A-Za-z0-9_-]{8}\.js$/,
  urls: [
    { value: "https://raw.githubusercontent.com/Meteor-Wallet/meteor_wallet_sdk/6369ad789d2f3aaf8e6ccb065021723111027174/storage/meteor-near-connect-latest.js", reason: "The Meteor Wallet code, pinned to one commit by the wallet list in the repo." },
    { value: "https://meteorwallet.app/", reason: "The Meteor Wallet website, which the wallet picker shows by its host name." },
    { value: "https://meteorwallet.app", reason: "A link the Meteor Wallet code may open, from its pinned allowsOpen list." },
    { value: "https://wallet.meteorwallet.app", reason: "The Meteor web wallet, which the wallet code may open as a pop-up." },
    { value: "https://chromewebstore.google.com", reason: "The extension store the Meteor Wallet code may open to install the extension." },
    { value: "https://raw.githubusercontent.com/hot-dao/near-selector/refs/heads/main/repository/manifest.json", reason: "The library's default wallet list; not fetched, since the page passes its own list." },
    { value: "https://cdn.jsdelivr.net/gh/azbang/hot-connector/repository/manifest.json", reason: "The library's second default wallet list; not fetched, since the page passes its own list." },
    { value: "https://wallet.near.org", reason: "The library's default footer link; not shown, since the page turns the footer off." },
    { value: "https://web.telegram.org", reason: "The message target the library uses only inside a Telegram mini app." },
    { value: "https://www.mynearwallet.com/images/webclip.png", reason: "The picture of the library's custom wallet form for wallet developers; not opened by the page." },
    { value: "https://github.com/azbang/hot-connector", reason: "A documentation link in the library's custom wallet form; not opened by the page." },
  ],
  values: [
    { value: "nightly.near", reason: "The name of the Nightly extension object in library code, not an account." },
    { value: "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz", reason: "The base58 alphabet of the library's encoder." },
    { value: "addFunctionCallKey:e.addFunctionCallKey", reason: "A property read in minified library code, not a key." },
    { value: "signInWithFunctionCallKey:e.addFunctionCallKey", reason: "A property read in minified library code, not a key." },
    { value: "publicKey:e.addKey.publicKey.toString", reason: "A property read in minified library code, not a key." },
    { value: "publicKey:e.deleteKey.publicKey.toString", reason: "A property read in minified library code, not a key." },
    { value: "publicKey:e.stake.publicKey.toString", reason: "A property read in minified library code, not a key." },
  ],
};

/** The accounts and hosts the page may name, from the fixed config. */
export function optionsFromConfig(env: Record<string, string | undefined> = process.env): ScanOptions {
  const networks: KnownNetwork[] = Object.values(KNOWN_NETWORKS);
  const root: HostRule = { path: /^\/?$/ };
  const allowedHosts: Record<string, HostRule> = {
    "fonts.googleapis.com": { path: /^\/(?:css2)?$/, queryKeys: ["family", "display"] },
    "fonts.gstatic.com": root,
    "www.w3.org": { path: /^\/2000\/svg$/ },
    // The alert bot link: that host and exactly that path, no query.
    [new URL(BOT_URL).hostname]: { path: new RegExp(`^${new URL(BOT_URL).pathname}/?$`) },
    // The faucet link of the how to try it box: that host and exactly that path, no query.
    [new URL(FAUCET_URL).hostname]: { path: new RegExp(`^${new URL(FAUCET_URL).pathname.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}/?$`) },
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
    ownTestAccounts: OWN_TEST_ACCOUNTS,
    evidenceTxHashes: evidenceTxHashes(),
    allowedHosts,
    chunkAllowances: [WALLET_CHUNK],
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
