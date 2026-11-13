// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { hostedEntryHref, networkBanner } from "../src/hosted-entry.ts";
import type { KnownNetwork } from "../src/known-deployments.ts";

const withDemo: Record<string, KnownNetwork> = {
  testnet: { rpcUrl: "https://rpc.example", fastNearUrl: "https://tx.example", factoryFromBlock: 0, factory: "f.testnet", codeHash: null, demoCoin: "demo.f.testnet", explorerTxUrl: "https://x.example/txns/", recordedRunFile: null },
};
const noDemo: Record<string, KnownNetwork> = { testnet: { ...withDemo.testnet!, demoCoin: null } };

describe("the fixed banner", () => {
  it("shows the fixed testnet text when the link names the testnet", () => {
    expect(networkBanner("https://fc.example/?network=testnet&coin=a.f.testnet")).toBe("Testnet. Read only. No wallet needed.");
  });
  it("shows nothing for a link with no network or another network", () => {
    expect(networkBanner("https://fc.example/")).toBeNull();
    expect(networkBanner("https://fc.example/?network=mainnet")).toBeNull();
  });
});

describe("the hosted entry link", () => {
  it("opens the demo coin of the testnet on a hosted host with no parameters", () => {
    expect(hostedEntryHref("https://fc.example/", withDemo)).toBe("?network=testnet&coin=demo.f.testnet");
  });
  it("leaves a link alone when it has any parameter", () => {
    expect(hostedEntryHref("https://fc.example/?view=coins", withDemo)).toBeNull();
    expect(hostedEntryHref("https://fc.example/?coin=x.f.testnet&network=testnet", withDemo)).toBeNull();
  });
  it("leaves a local host alone, so the local checks still open the landing screen", () => {
    expect(hostedEntryHref("http://127.0.0.1:5173/", withDemo)).toBeNull();
    expect(hostedEntryHref("http://localhost:5173/", withDemo)).toBeNull();
  });
  it("leaves the page alone until the config names a demo coin", () => {
    expect(hostedEntryHref("https://fc.example/", noDemo)).toBeNull();
  });
});

describe("drawing the banner", () => {
  it("puts the banner before the app, once, as plain text", async () => {
    const { showNetworkBanner } = await import("../src/hosted-entry.ts");
    document.body.innerHTML = '<main id="app"></main>';
    showNetworkBanner(document, "https://fc.example/?network=testnet&coin=a.f.testnet");
    showNetworkBanner(document, "https://fc.example/?network=testnet&coin=a.f.testnet");
    const banners = document.querySelectorAll("#network-banner");
    expect(banners).toHaveLength(1);
    expect(banners[0]!.textContent).toBe("Testnet. Read only. No wallet needed.");
    expect(banners[0]!.nextElementSibling?.id).toBe("app");
  });
  it("draws nothing for a link with no network", async () => {
    const { showNetworkBanner } = await import("../src/hosted-entry.ts");
    document.body.innerHTML = '<main id="app"></main>';
    showNetworkBanner(document, "https://fc.example/");
    expect(document.querySelector("#network-banner")).toBeNull();
  });
});
