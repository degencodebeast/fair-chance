import { describe, expect, it } from "vitest";
import { configFromUrl } from "../src/chain.ts";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import { resolvePageLink } from "../src/page-link.ts";

const coin = "moth.factory.testnet";
const evilAnchors = "&factory=evil.testnet&code_hash=EvilHash1111111111111111111111111111111111";
const link = (query: string) => `http://localhost/?coin=${coin}&${query}`;
const enc = encodeURIComponent;

describe("the page link picks only the network", () => {
  it("uses the fixed testnet URLs and ignores an RPC URL with a testnet label", () => {
    const r = resolvePageLink(link(`network=testnet&rpc=${enc("https://rpc.testnet.evil.example")}&fastnear=${enc("https://evil.example")}${evilAnchors}`));
    expect(r.rpcUrl).toBe(KNOWN_NETWORKS.testnet.rpcUrl);
    expect(r.fastNearUrl).toBe(KNOWN_NETWORKS.testnet.fastNearUrl);
    expect(r.known).toEqual({ factory: KNOWN_NETWORKS.testnet.factory, codeHash: KNOWN_NETWORKS.testnet.codeHash });
    expect(r.rpcHost).toBe(new URL(KNOWN_NETWORKS.testnet.rpcUrl).host);
  });

  it.each([
    "https://rpc.testnet.evil.example",
    "http://localhost.evil.example",
    "http://127.0.0.1.evil.example:1",
    "http://127.0.0.1@evil.example",
    "http://localhost:3030@evil.example",
    "ftp://localhost",
  ])("gives no RPC URL and no trusted values for the link RPC %s without a network", (rpc) => {
    expect(() => resolvePageLink(link(`rpc=${enc(rpc)}${evilAnchors}`))).toThrow(/network=testnet/);
  });

  it.each(["http://localhost.evil.example", "http://127.0.0.1.evil.example:1", "http://127.0.0.1@evil.example"])(
    "uses the fixed URLs and fixed anchors for the link RPC %s with a network",
    (rpc) => {
      const r = resolvePageLink(link(`network=testnet&rpc=${enc(rpc)}${evilAnchors}`));
      expect(r.rpcUrl).toBe(KNOWN_NETWORKS.testnet.rpcUrl);
      expect(r.known.factory).toBe(KNOWN_NETWORKS.testnet.factory);
      expect(r.known.codeHash).toBe(KNOWN_NETWORKS.testnet.codeHash);
    },
  );

  it("refuses a network the page does not know", () => {
    expect(() => resolvePageLink(link("network=mainnet"))).toThrow(/network/);
  });
});

describe("a local sandbox may give the RPC URL, factory and code hash", () => {
  it.each(["http://localhost:3030", "http://127.0.0.1:3030"])("takes them for %s", (rpc) => {
    const r = resolvePageLink(link(`rpc=${enc(rpc)}&factory=factory.sandbox&code_hash=Hash111`));
    expect(r.rpcUrl).toBe(rpc);
    expect(r.known).toEqual({ factory: "factory.sandbox", codeHash: "Hash111" });
    expect(r.rpcHost).toBe(new URL(rpc).host);
  });

  it("keeps the old shape for configFromUrl", () => {
    expect(configFromUrl(link(`rpc=${enc("http://127.0.0.1:53021")}`))).toEqual({ coin, rpcUrl: "http://127.0.0.1:53021" });
  });

  it("takes a link FastNEAR URL only for a local host", () => {
    const sandbox = enc("http://localhost:3030");
    expect(resolvePageLink(link(`rpc=${sandbox}&fastnear=${enc("http://127.0.0.1:4040")}`)).fastNearUrl).toBe("http://127.0.0.1:4040");
    for (const bad of ["https://evil.example", "http://localhost.evil.example", "http://127.0.0.1@evil.example"]) {
      expect(resolvePageLink(link(`rpc=${sandbox}&fastnear=${enc(bad)}`)).fastNearUrl).toBeUndefined();
    }
  });

  it("does not let a network link pick local anchors", () => {
    const r = resolvePageLink(link(`network=testnet${evilAnchors}`));
    expect(r.known.factory).toBe(KNOWN_NETWORKS.testnet.factory);
  });
});
