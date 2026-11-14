// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { fallbackList } from "../src/list-fallback.ts";
import { SAMPLE_COINS } from "../src/coin-list-sample.ts";
import { resolveListLink } from "../src/page-link.ts";
import { drawLanding } from "../src/render-landing.ts";

describe("the coin list with nothing to read", () => {
  it("keeps the made-up coins on the local machine", () => {
    expect(fallbackList({ kind: "sample" }, "localhost").cards).toBe(SAMPLE_COINS);
    expect(fallbackList({ kind: "no-factory", source: "NEAR testnet" }, "127.0.0.1").cards).toBe(SAMPLE_COINS);
  });
  it("shows no made-up coin on a hosted host, and says why", () => {
    for (const link of [{ kind: "sample" }, { kind: "no-factory", source: "NEAR testnet" }] as const) {
      const out = fallbackList(link, "fc.example");
      expect(out.cards).toEqual([]);
      expect(out.note).not.toMatch(/sample|made up/i);
    }
  });
});

describe("a hosted link with no network", () => {
  it("reads the testnet chain, not sample data", () => {
    const link = resolveListLink("https://fc.example/?view=coins");
    expect(link.kind).toBe("chain");
  });
  it("still gives sample data on the local machine", () => {
    expect(resolveListLink("http://localhost:5173/?view=coins").kind).toBe("sample");
  });
});

describe("the landing screen", () => {
  const text = (href: string) => {
    document.body.innerHTML = '<main id="app"></main>';
    drawLanding(document.querySelector("#app")!, href);
    return document.body.textContent ?? "";
  };
  it("has no sample data label on a hosted host", () => {
    expect(document.body.innerHTML).toBeDefined();
    const t = text("https://fc.example/?network=testnet");
    expect(t).not.toMatch(/sample data/i);
    expect(document.querySelector("#proof")).toBeNull();
  });
  it("keeps the labeled example receipt on the local machine", () => {
    expect(text("http://localhost:5173/")).toMatch(/Sample data/);
  });
});
