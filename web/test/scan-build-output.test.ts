import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { optionsFromConfig, scanBuildOutput, type ScanOptions } from "../scripts/scan-build-output.ts";
import { FAUCET_URL } from "../src/try-it-copy.ts";
import { WALLET_MANIFEST } from "../src/wallet-manifest.ts";
import { landingCopy } from "../src/landing-copy.ts";

// Every bad value below is an obvious fake planted in a temporary folder. Parts are joined at run
// time so this file holds no private-looking string of its own.
let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "fc-scan-"));
  mkdirSync(join(dir, "assets"));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

const options: ScanOptions = {
  allowedAccounts: ["fake-factory.testnet", "demo.fake-factory.testnet"],
  allowedHosts: {
    "rpc.fake-public.example": { path: /^\/?$/ },
    "tx.fake-public.example": { path: /^\/?$/ },
    "fonts.googleapis.com": { path: /^\/(?:css2)?$/, queryKeys: ["family", "display"] },
    "www.w3.org": { path: /^\/2000\/svg$/ },
  },
  providerHosts: [],
  sampleAccounts: ["sample-one.testnet"],
  allowedValues: ["Rv9TbK4pWz8NcH3sYd6FgJ1aUeQ7xLm2"],
};

function plant(name: string, text: string): void {
  writeFileSync(join(dir, name), text);
}
const rules = async (extra: Partial<ScanOptions> = {}) => (await scanBuildOutput(dir, { ...options, ...extra })).map((f) => f.rule).sort();

describe("the pre-deploy scan of the build", () => {
  it("passes a clean build that names only the known accounts and hosts", async () => {
    plant("index.html", '<link href="https://fonts.googleapis.com/css2?family=X"><svg xmlns="http://www.w3.org/2000/svg"/>');
    plant("assets/app.js", 'const a="https://rpc.fake-public.example",b="https://tx.fake-public.example",c="demo.fake-factory.testnet",d="x.demo.fake-factory.testnet";');
    expect(await rules()).toEqual([]);
  });

  it("fails on a wallet that is not a known account, and on a 64-character implicit account", async () => {
    plant("assets/app.js", `const w="someone-else.testnet",m="who.near",i="${"ab".repeat(32)}";`);
    expect(await rules()).toEqual(["wallet", "wallet", "wallet"]);
  });

  it("allows a listed sample account only by its exact name", async () => {
    plant("assets/app.js", 'const a="sample-one.testnet",b="x.sample-one.testnet";');
    expect(await rules()).toEqual(["wallet"]);
  });

  it("does not read a short minified property access in a script as a wallet, but a quoted name is one", async () => {
    plant("assets/app.js", "const r=t.testnet.rpcUrl;");
    expect(await rules()).toEqual([]);
    plant("assets/b.js", 'const w="t.testnet";');
    expect(await rules()).toEqual(["wallet"]);
  });

  it("fails on a workspace or home folder path", async () => {
    plant("assets/app.js", `const p="/${"Us"}ers/someone/project",q="/${"ho"}me/someone/x",r="C:\\\\${"Us"}ers\\\\x",s="${"near"}-${"hack"}",t=".${"omc"}/x",u="/${"private"}/var/x";`);
    expect(new Set(await rules())).toEqual(new Set(["workspace-path"]));
    expect((await rules()).length).toBe(6);
  });

  it("fails on a key or token", async () => {
    plant("assets/app.js", `const k="${"ed25519"}:${"A".repeat(50)}",s="${"secp256k1"}:${"B".repeat(50)}",h="Authorization: Bearer ${"c".repeat(30)}",p="-----BEGIN ${"PRIVATE"} KEY-----";`);
    expect(await rules()).toEqual(["key", "key", "key", "key"]);
  });

  it("fails on a private endpoint: a host that is not allowed, a key in a URL, or a login in a URL", async () => {
    plant("assets/a.js", 'fetch("https://my-private-rpc.fake-private.example/v1/path");');
    plant("assets/b.js", 'fetch("https://rpc.fake-public.example/?api_key=FAKEFAKEFAKE");');
    plant("assets/c.js", 'fetch("https://user:pass@rpc.fake-public.example/");');
    expect(await rules()).toEqual(["private-endpoint", "private-endpoint", "private-endpoint"]);
  });

  it("fails on the host name of a keyed RPC provider, from the built-in list and from the extra list", async () => {
    plant("assets/a.js", 'const u="https://near-mainnet.infura.io/v3/x";');
    plant("assets/b.js", 'const u="https://rpc.fake-keyed-provider.example/";');
    const found = await scanBuildOutput(dir, { ...options, providerHosts: ["fake-keyed-provider.example"] });
    expect(found.map((f) => f.rule).sort()).toEqual(["private-endpoint", "private-endpoint", "provider-host", "provider-host"]);
  });

  it("finds a provider host name that is not a full URL", async () => {
    plant("assets/a.js", 'const h="rpc.fake-keyed-provider.example";');
    expect(await rules({ providerHosts: ["fake-keyed-provider.example"] })).toEqual(["provider-host"]);
  });

  it("names the file and the rule but never prints the value it found", async () => {
    const secret = `${"ed25519"}:${"Z".repeat(50)}`;
    plant("assets/app.js", `const k="${secret}";`);
    const found = await scanBuildOutput(dir, options);
    expect(found).toHaveLength(1);
    expect(found[0]!.file).toBe("assets/app.js");
    expect(JSON.stringify(found)).not.toContain(secret);
    expect(JSON.stringify(found)).not.toContain("ZZZZZZZZ");
  });

  it("scans every file in nested folders, including a source map", async () => {
    mkdirSync(join(dir, "assets", "deep"));
    plant("assets/deep/app.js.map", '{"sources":["/Us' + 'ers/someone/x.ts"]}');
    expect(await rules()).toEqual(["workspace-path"]);
  });

  it("fails when the folder does not exist, so a missing build is never a pass", async () => {
    await expect(scanBuildOutput(join(dir, "missing"), options)).rejects.toThrow();
  });

  const host = ["rpc", "private-fake", "example"].join(".");
  it.each([
    ["an uppercase scheme", "assets/a.js", `fetch("HTTPS://${host}/rpc")`],
    ["a protocol-relative URL", "assets/a.js", `fetch("//${host}/rpc")`],
    ["a wss URL", "assets/a.js", `new WebSocket("wss://${host}/rpc")`],
    ["a ws URL", "assets/a.js", `new WebSocket("ws://${host}/rpc")`],
    ["a JSON-escaped URL", "run.json", `{"src":"https:\\/\\/${host}\\/rpc"}`],
    ["a key in the path of an allowed host", "assets/a.js", `fetch("https://rpc.fake-public.example/v1/${"Ab1".repeat(10)}")`],
    ["a key in the query of an allowed host with a name after a semicolon", "assets/a.js", 'fetch("https://rpc.fake-public.example/;x-api-key=FAKEFAKEFAKE")'],
  ])("fails on a private endpoint written as %s", async (_name, file, text) => {
    plant(file, text);
    expect(await rules()).toEqual(["private-endpoint"]);
  });

  it.each([
    ["an x-api-key header written as a pair", `const h="x-api-key=${"Q".repeat(24)}";`],
    ["an x-api-key header written as an object field", `const h={"x-api-key":"${"Q".repeat(24)}"};`],
    ["an apiKey object field", `const c={apiKey:"FAKE${"Z".repeat(30)}"};`],
    ["a JWT", `const j="${"eyJ"}hbGciOiJIUzI1NiJ9.${"eyJ"}zdWIiOiJmYWtlIn0.c2lnbmF0dXJl";`],
    ["a GitHub token", `const g="${"ghp"}_${"a1B2".repeat(9)}";`],
    ["a GitHub fine-grained token", `const g="${"github"}_pat_${"a1B2c3".repeat(5)}";`],
    ["a long bare high-entropy string", 'const t="aB3dE5gH7jK9mN1pQ3sT5vX7zA9cE1fG3hJ5";'],
  ])("fails on a key written as %s", async (_name, text) => {
    plant("assets/a.js", text);
    expect(await rules()).toEqual(["key"]);
  });

  it("does not call a long plain word or a repeated pattern a key", async () => {
    plant("assets/a.js", `const a="${"abc".repeat(14)}",b="internationalization-and-localization-settings",c="${"0".repeat(40)}";`);
    expect(await rules()).toEqual([]);
  });

  it("does not call the exact name of a build file a key, in the name or in an import path", async () => {
    plant("assets/render-very-long-screen-name-that-goes-on-and-on-Bh2fty9r.js", 'import("./render-very-long-screen-name-that-goes-on-and-on-Bh2fty9r.js");import("assets/render-very-long-screen-name-that-goes-on-and-on-Bh2fty9r.js");');
    plant("assets/index-B--492nd.js", "x");
    expect(await rules()).toEqual([]);
  });

  it.each([
    ["words then a key", "my-api-"],
    ["a path then a key", "x/y/"],
    ["live-key then a key", "live-key-"],
    ["two words then a key", "app-core-"],
    ["an assets path then a key that is not a build file", "assets/render-coin-page-"],
  ])("still flags %s", async (_name, front) => {
    plant("assets/a.js", `const s="${front}${"Q7xLm2Rv9TbK4pWz8NcH3sYd6FgJ1aUe"}";`);
    expect(await rules()).toEqual(["key"]);
  });

  it("fails on a short wallet name inside a string, and on 0x accounts", async () => {
    plant("assets/a.js", `const a="x ab.near",b="owner me.testnet",c="0x${"ab".repeat(20)}";`);
    expect(await rules()).toEqual(["wallet", "wallet", "wallet"]);
  });

  it("reads real short property reads in a script as code", async () => {
    plant("assets/a.js", "const r=t.testnet.rpcUrl;f(n.testnet);return o.testnet;");
    expect(await rules()).toEqual([]);
  });

  it("scans the file name too and prints a found name only masked", async () => {
    plant("fake-alice.testnet.json", "{}");
    const found = await scanBuildOutput(dir, options);
    expect(found.map((f) => f.rule)).toEqual(["wallet"]);
    expect(found[0]!.file).toBe("fak... (23 characters)");
    expect(JSON.stringify(found)).not.toContain("fake-alice");
  });

  it.each([
    ["a 32-character hex key", `const s="${"9f3a7c2e5b8d1f4a".repeat(2)}";`],
    ["a 40-character hex key", `const s="${"9f3a7c2e5b".repeat(4)}";`],
    ["a UUID", 'const s="9f3a7c2e-5b8d-4f4a-8c0e-2b7d9a3f5c8e";'],
    ["a 24-character mixed key", 'const s="Q7xLm2Rv9TbK4pWz8NcH3sYd";'],
    ["an all-uppercase key", 'const s="QXLMRVTBKPWZNCHSYDFGJAUEQXLMRVTB";'],
    ["a sk_live key", `const s="${"sk"}_live_${"a1B2c3D4"}";`],
    ["an AWS key id", `const s="${"AKIA"}${"ABCDEFGHIJKLMNOP"}";`],
    ["an rpcKey field", 'const c={rpcKey:"fakefakefakefakefake"};'],
    ["a field that ends in token", 'const c={sessionToken:"fakefakefakefakefake"};'],
    ["a field that ends in secret", 'const c={clientSecret:"fakefakefakefakefake"};'],
  ])("fails on %s", async (_name, text) => {
    plant("assets/a.js", text);
    expect(await rules()).toEqual(["key"]);
  });

  it("allows the exact known values and nothing near them", async () => {
    plant("assets/a.js", 'const a="Rv9TbK4pWz8NcH3sYd6FgJ1aUeQ7xLm2";');
    expect(await rules()).toEqual([]);
    plant("assets/b.js", 'const a="Rv9TbK4pWz8NcH3sYd6FgJ1aUeQ7xLm2X";');
    expect(await rules()).toEqual(["key"]);
  });

  it.each([
    ["a query key the page does not use on the font host", 'fetch("https://fonts.googleapis.com/css2?k=FAKE")'],
    ["a port on an allowed host", 'fetch("https://rpc.fake-public.example:8443/")'],
    ["a protocol-relative host with no dot", 'fetch("//intranet/x")'],
    ["a protocol-relative localhost with a port", 'fetch("//localhost:8080/x")'],
  ])("fails on a private endpoint written as %s", async (_name, text) => {
    plant("assets/a.js", text);
    expect(await rules()).toEqual(["private-endpoint"]);
  });

  it("allows the font style sheet with only the keys the page uses", async () => {
    plant("index.html", '<link href="https://fonts.googleapis.com/css2?family=A+B&family=C:wght@400&display=swap">');
    expect(await rules()).toEqual([]);
  });

  it.each([
    ["in parentheses", 'const s="paid to (me.testnet)";'],
    ["after a colon", 'const s="by:jo.near,";'],
    ["in a template", "const s=`to ${x} me.testnet`;"],
    ["in a single-quoted string after a bracket", "const s='[me.near]';"],
  ])("fails on a short wallet name inside a string, %s", async (_name, text) => {
    plant("assets/a.js", text);
    expect(await rules()).toEqual(["wallet"]);
  });

  it("still reads a short property in code, also inside a template expression", async () => {
    plant("assets/a.js", "const s=`rpc ${t.testnet.rpcUrl}`;const q=(n.testnet);");
    expect(await rules()).toEqual([]);
  });

  it.each([
    ["a regex literal with a double quote", 'const r=/"/;const s="paid to me.testnet";'],
    ["the real regex of the refusal texts", 'const a=b.match(/^[^\\s"\']*/);const s="paid to me.testnet";'],
    ["a regex after return", 'function f(x){return /\'/.test(x)}const s="paid to me.testnet";'],
    ["a regex with a slash in a class", 'const r=/[/"]x/g;const s="paid to me.testnet";'],
    ["a regex after a comma and a colon", 'f(1,/"/);const o={a:/\'/};const s="paid to me.testnet";'],
  ])("fails on a short wallet name in a string after %s", async (_name, text) => {
    plant("assets/a.js", text);
    expect(await rules()).toEqual(["wallet"]);
  });

  it("does not take a division for a regex", async () => {
    plant("assets/a.js", 'const x=(a+b)/2,y=c[1]/3,z=a/b/c;const s="paid to me.testnet";');
    expect(await rules()).toEqual(["wallet"]);
  });

  it.each([
    ["an unclosed string", 'const s="paid to me.testnet;\nconst t=1;'],
    ["an unclosed template", "const s=`paid to ${x} me.testnet;"],
    ["an unclosed comment", "const s=1;/* me.testnet"],
    ["an unclosed regex", "const r=/abc;\nconst t=1;"],
  ])("does not pass a script it cannot read: %s", async (_name, text) => {
    plant("assets/a.js", text);
    expect(await rules()).toContain("unreadable-file");
  });

  it("allows only names of the shape the build emits, so a key is not allowed by being a file name", async () => {
    const key = "Q7xLm2Rv9TbK4pWz8NcH3sYd6FgJ1aUe";
    plant("index.html", "<html></html>");
    plant("assets/page-link-H3g8Fd-O.js", `import("./page-link-H3g8Fd-O.js");import("assets/page-link-H3g8Fd-O");`);
    plant("assets/index-Cj8UrBYp.css", "a{}");
    expect(await rules()).toEqual([]);
    plant("assets/a.js", `const s="${key}";`);
    plant(`assets/${key}.txt`, "x");
    // The key is found in the script, and its own file name is scanned as text.
    expect((await rules()).filter((r) => r === "key")).toHaveLength(2);
  });

  it("scans a file name that is only a key", async () => {
    plant(`assets/${"Q7xLm2Rv9TbK4pWz8NcH3sYd6FgJ1aUe"}.txt`, "x");
    expect(await rules()).toEqual(["key"]);
  });

  it("does not allow a key as the word part or the hash of a build-shaped name", async () => {
    plant("assets/Q7xLm2Rv9TbK4pWz8NcH3sYd6FgJ1aUe-Cj8UrBYp.js", "x");
    plant("assets/page-link-Q7xLm2Rv9TbK4pWz8NcH3sYd6FgJ1aUe.js", "x");
    expect(await rules()).toEqual(["key", "key"]);
  });
});

describe("the faucet link", () => {
  it("passes in the build with the real config, and no other faucet path does", async () => {
    plant("assets/app.js", `const f="${FAUCET_URL}";`);
    expect(await scanBuildOutput(dir, optionsFromConfig({}))).toEqual([]);
    plant("assets/app.js", `const f="${FAUCET_URL}?key=abc";`);
    expect((await scanBuildOutput(dir, optionsFromConfig({}))).map((f) => f.rule)).toContain("private-endpoint");
    plant("assets/app.js", 'const f="https://docs.near.org/other";');
    expect((await scanBuildOutput(dir, optionsFromConfig({}))).map((f) => f.rule)).toContain("private-endpoint");
    plant("assets/app.js", 'const f="https://near-faucet.io/";');
    expect((await scanBuildOutput(dir, optionsFromConfig({}))).map((f) => f.rule)).toContain("private-endpoint");
  });
});

describe("the bot link", () => {
  it("passes for exactly the bot path and no other t.me path or query", async () => {
    const bot = "https://t.me/fairchance_alert_bot";
    plant("assets/app.js", `const b="${bot}";`);
    expect(await scanBuildOutput(dir, optionsFromConfig({}))).toEqual([]);
    for (const other of [`${bot}?start=x`, `${bot}2`, "https://t.me/other_bot", "https://t.me/", `${bot}/extra`]) {
      plant("assets/app.js", `const b="${other}";`);
      expect((await scanBuildOutput(dir, optionsFromConfig({}))).map((f) => f.rule), other).toContain("private-endpoint");
    }
  });
});

describe("the repository link of the landing footer", () => {
  it("passes for exactly that repository and no other GitHub path, query or host", async () => {
    const repo = landingCopy.githubUrl;
    expect(repo).toBe("https://github.com/degencodebeast/fair-chance");
    plant("assets/render-landing-AbCd1234.js", `const g="${repo}";`);
    expect(await scanBuildOutput(dir, optionsFromConfig({}))).toEqual([]);
    plant("assets/render-landing-AbCd1234.js", `const g="${repo}/";`);
    expect(await scanBuildOutput(dir, optionsFromConfig({}))).toEqual([]);
    for (const other of [
      "https://github.com/degencodebeast/other-repo",
      "https://github.com/someone-else/fair-chance",
      `${repo}/blob/main/README.md`,
      `${repo}?tab=readme`,
      `${repo}2`,
      "https://github.com/",
      "https://github.com.evil.example/degencodebeast/fair-chance",
      "https://gist.github.com/degencodebeast/fair-chance",
    ]) {
      plant("assets/render-landing-AbCd1234.js", `const g="${other}";`);
      expect((await scanBuildOutput(dir, optionsFromConfig({}))).map((f) => f.rule), other).toContain("private-endpoint");
    }
  });
});

describe("the allow list of one chunk", () => {
  const lib = "https://lib-host.example/docs/manifest.json";
  const chunk: ScanOptions["chunkAllowances"] = [
    {
      file: /^assets\/wallet-lib-[A-Za-z0-9_-]{8}\.js$/,
      urls: [{ value: lib, reason: "a default the library does not use" }, { value: "https://wallet.other-host.example", reason: "a link" }],
      values: [{ value: "plugin.near", reason: "an extension object name in library code" }],
    },
  ];
  const withChunk = () => rules({ chunkAllowances: chunk });

  it("passes the exact listed values inside the named chunk", async () => {
    plant("assets/wallet-lib-AbCd1234.js", `const a="${lib}",b="plugin.near",c="https://wallet.other-host.example";`);
    expect(await withChunk()).toEqual([]);
  });

  it("still flags the same values in any other chunk", async () => {
    plant("assets/index-AbCd1234.js", `const a="${lib}",b="plugin.near";`);
    expect(await withChunk()).toEqual(["private-endpoint", "wallet"]);
  });

  it("still flags a value that is not exactly the listed one, inside the chunk", async () => {
    plant("assets/wallet-lib-AbCd1234.js", `const a="${lib}?key=abc",b="x.plugin.near",c="https://lib-host.example/other";`);
    expect(await withChunk()).toEqual(["private-endpoint", "private-endpoint", "wallet"]);
  });

  it("matches the chunk by its whole name only", async () => {
    plant("assets/wallet-lib-AbCd1234.js.txt", `const b="plugin.near";`);
    plant("assets/my-wallet-lib-AbCd1234.js", `const b="plugin.near";`);
    expect(await withChunk()).toEqual(["wallet", "wallet"]);
  });
});

describe("the real allow list of the wallet library chunk", () => {
  const real = optionsFromConfig({}).chunkAllowances ?? [];

  it("names one chunk, the wallet adapter, by its build name", () => {
    expect(real).toHaveLength(1);
    expect(real[0]!.file.test("assets/wallet-adapter-NuRaRLKp.js")).toBe(true);
    expect(real[0]!.file.test("assets/index-NuRaRLKp.js")).toBe(false);
    expect(real[0]!.file.test("assets/wallet-adapter-NuRaRLKp.css")).toBe(false);
  });

  it("gives each entry a one-line reason", () => {
    for (const e of [...real[0]!.urls, ...real[0]!.values]) {
      expect(e.reason.length, e.value).toBeGreaterThan(10);
      expect(e.reason).not.toContain("\n");
    }
  });

  it("holds every link of the pinned wallet manifest, so the scan passes only the wallet the repo pins", () => {
    const urls = real[0]!.urls.map((u) => u.value);
    for (const w of WALLET_MANIFEST.wallets) {
      expect(urls).toContain(w.executor);
      for (const open of w.permissions.allowsOpen ?? []) if (open.startsWith("https://")) expect(urls).toContain(open);
    }
  });
});
