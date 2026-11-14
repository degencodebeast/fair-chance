// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { copyViolations } from "./copy-rules.ts";
import { hostedEntryHref, networkBanner, TESTNET_BANNER } from "../src/hosted-entry.ts";

describe("the fixed banner", () => {
  it("shows the fixed testnet text when the link names the testnet", () => {
    expect(networkBanner("https://fc.example/?network=testnet")).toBe("NEAR testnet. Test tokens have no value. No audit.");
  });
  it("shows nothing for a link with no network or another network", () => {
    expect(networkBanner("https://fc.example/")).toBeNull();
    expect(networkBanner("https://fc.example/?network=mainnet")).toBeNull();
  });
});

describe("the banner text", () => {
  it("is a short true sentence that follows the copy rules and promises nothing", () => {
    expect(copyViolations(TESTNET_BANNER)).toEqual([]);
    expect(TESTNET_BANNER).not.toMatch(/production|safe|read only|no wallet/i);
  });
});

describe("the hosted entry link", () => {
  it("names the testnet and no coin, so the landing screen opens", () => {
    expect(hostedEntryHref("https://fc.example/")).toBe("?network=testnet");
  });
  it("leaves a link alone when it has any parameter", () => {
    expect(hostedEntryHref("https://fc.example/?view=coins")).toBeNull();
    expect(hostedEntryHref("https://fc.example/?coin=x.f.testnet&network=testnet")).toBeNull();
  });
  it.each(["http://127.0.0.1:5173/", "http://localhost:5173/", "http://[::1]:5173/", "http://0.0.0.0:5173/", "http://app.localhost:5173/", "http://127.0.0.2:5173/", "http://127.255.255.254/", "http://[::ffff:127.0.0.1]:5173/", "http://LOCALHOST.:5173/", "http://[0:0:0:0:0:0:0:1]:5173/"])("leaves the local host %s alone", (href) => {
    expect(hostedEntryHref(href)).toBeNull();
  });
  it.each(["https://localhost.evil.example/", "https://127.0.0.1.evil.example/", "https://128.0.0.1/", "https://notlocalhost/"])("still adds the network on the hosted host %s", (href) => {
    expect(hostedEntryHref(href)).toBe("?network=testnet");
  });
});

describe("drawing the banner", () => {
  it("puts the banner before the app, once, as plain text", async () => {
    const { showNetworkBanner } = await import("../src/hosted-entry.ts");
    document.body.innerHTML = '<main id="app"></main>';
    showNetworkBanner(document, "https://fc.example/?network=testnet");
    showNetworkBanner(document, "https://fc.example/?network=testnet");
    const banners = document.querySelectorAll("#network-banner");
    expect(banners).toHaveLength(1);
    expect(banners[0]!.textContent).toBe("NEAR testnet. Test tokens have no value. No audit.");
    expect(banners[0]!.nextElementSibling?.id).toBe("app");
  });
  it("draws nothing for a link with no network", async () => {
    const { showNetworkBanner } = await import("../src/hosted-entry.ts");
    document.body.innerHTML = '<main id="app"></main>';
    showNetworkBanner(document, "https://fc.example/");
    expect(document.querySelector("#network-banner")).toBeNull();
  });
});
