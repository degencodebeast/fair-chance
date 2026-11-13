import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { scanBuildOutput, type ScanOptions } from "../scripts/scan-build-output.ts";

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
  allowedHosts: ["rpc.fake-public.example", "tx.fake-public.example", "fonts.googleapis.com", "www.w3.org"],
  providerHosts: [],
  sampleAccounts: ["sample-one.testnet"],
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
});
