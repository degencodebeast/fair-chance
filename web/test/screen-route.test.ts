import { describe, expect, it } from "vitest";
import { screenFor, screenLink } from "../src/screen-route.ts";

const base = "http://localhost/";

describe("which screen a page link opens", () => {
  it("opens the landing screen with no parameters", () => {
    expect(screenFor(base)).toBe("landing");
  });
  it("opens the coin list for view=coins and the alerts screen for view=alerts", () => {
    expect(screenFor(`${base}?view=coins`)).toBe("coins");
    expect(screenFor(`${base}?view=alerts`)).toBe("alerts");
  });
  it("opens the coin page whenever a coin is named, whatever the view says", () => {
    expect(screenFor(`${base}?coin=moth.factory.testnet&network=testnet`)).toBe("coin");
    expect(screenFor(`${base}?view=coins&coin=moth.factory.testnet`)).toBe("coin");
  });
  it("keeps the alerts screen when a coin is named, because the alerts link its receipts to that coin", () => {
    expect(screenFor(`${base}?view=alerts&coin=moth.factory.testnet&network=testnet`)).toBe("alerts");
  });
  it("falls back to the landing screen for a view it does not know", () => {
    expect(screenFor(`${base}?view=nothing`)).toBe("landing");
  });
});

describe("links between screens", () => {
  it("carries only the parameters that pick where the page reads from", () => {
    const from = `${base}?view=coins&network=testnet&rpc=http%3A%2F%2F127.0.0.1%3A3030&factory=f.test&code_hash=abc&fastnear=http%3A%2F%2F127.0.0.1%3A1&receipt=r1&q=moth`;
    const link = new URL(screenLink(from, "alerts"), base);
    expect(link.searchParams.get("view")).toBe("alerts");
    expect([...link.searchParams.keys()].sort()).toEqual(["code_hash", "factory", "fastnear", "network", "rpc", "view"]);
  });
  it("links the landing screen with no view parameter", () => {
    expect(screenLink(`${base}?view=coins&network=testnet`, "landing")).toBe("?network=testnet");
    expect(screenLink(base, "landing")).toBe("?");
  });
  it("links a coin page with the coin and an optional receipt", () => {
    const link = new URL(screenLink(`${base}?view=coins&network=testnet`, "coin", { coin: "moth.factory.testnet", receipt: "R1" }), base);
    expect(link.searchParams.get("coin")).toBe("moth.factory.testnet");
    expect(link.searchParams.get("receipt")).toBe("R1");
    expect(link.searchParams.get("network")).toBe("testnet");
    expect(link.searchParams.has("view")).toBe(false);
  });
});
