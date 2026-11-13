import { describe, expect, it } from "vitest";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import { resolveListLink } from "../src/page-link.ts";

const enc = encodeURIComponent;
const at = (query: string) => `http://localhost/?view=coins&${query}`;
const evil = `&factory=evil.testnet&code_hash=EvilHash1111111111111111111111111111111111`;

describe("the coin list link picks only the network", () => {
  it("reads no chain when the link names no network and no local RPC", () => {
    expect(resolveListLink("http://localhost/?view=coins")).toEqual({ kind: "sample" });
  });

  it("uses the fixed testnet URLs and the fixed factory, and ignores a link factory", () => {
    const r = resolveListLink(at(`network=testnet&rpc=${enc("https://rpc.evil.example")}${evil}`));
    expect(r.kind).toBe(KNOWN_NETWORKS.testnet.factory === null ? "no-factory" : "chain");
    if (r.kind === "chain") {
      expect(r.rpcUrl).toBe(KNOWN_NETWORKS.testnet.rpcUrl);
      expect(r.factory).toBe(KNOWN_NETWORKS.testnet.factory);
    }
    expect(JSON.stringify(r)).not.toContain("evil");
  });

  it.each(["https://rpc.testnet.evil.example", "http://localhost.evil.example", "http://127.0.0.1@evil.example"])(
    "refuses the link RPC %s without a network",
    (rpc) => {
      expect(() => resolveListLink(at(`rpc=${enc(rpc)}${evil}`))).toThrow(/network=testnet/);
    },
  );

  it("takes the RPC URL, the factory and the first block from the link for a local sandbox", () => {
    const r = resolveListLink(at(`rpc=${enc("http://127.0.0.1:3030")}&factory=factory.test.near&from_block=12`));
    expect(r).toEqual({
      kind: "chain",
      rpcUrl: "http://127.0.0.1:3030",
      rpcHost: "127.0.0.1:3030",
      factory: "factory.test.near",
      fromBlock: 12,
      source: "the local sandbox",
    });
  });

  it("starts at block 0 on a local sandbox when the link gives no first block", () => {
    const r = resolveListLink(at(`rpc=${enc("http://localhost:3030")}&factory=factory.test.near`));
    expect(r).toMatchObject({ kind: "chain", fromBlock: 0 });
  });

  it("reports a local sandbox with no factory as having no factory", () => {
    expect(resolveListLink(at(`rpc=${enc("http://localhost:3030")}`))).toEqual({ kind: "no-factory", source: "the local sandbox" });
  });
});
