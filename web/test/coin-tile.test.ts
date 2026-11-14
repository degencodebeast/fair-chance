// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { contrastRatio, drawCoinTile, TILE_PALETTE, tileColors, tileText } from "../src/coin-tile.ts";

const validIcon = "data:image/webp;base64,UklGRgAAAA==";

describe("the two characters", () => {
  it.each([
    ["CABAL", "CA"],
    ["CABAL2", "C2"],
    ["BAG", "BA"],
    ["bag", "BA"],
    ["X", "X"],
    ["", "?"],
  ])("%s gives %s", (ticker, text) => expect(tileText(ticker)).toBe(text));
});

describe("the colors", () => {
  it("are the same for the same coin account", () => {
    expect(tileColors("bag.factory.fairchance.testnet")).toEqual(tileColors("bag.factory.fairchance.testnet"));
  });
  it("differ for bag, cabal and cabal2", () => {
    const colors = ["bag", "cabal", "cabal2"].map((n) => tileColors(`${n}.factory.fairchance.testnet`).background);
    expect(new Set(colors).size).toBe(3);
  });
  it("keep a text contrast of at least 4.5 to 1 for every palette entry", () => {
    expect(TILE_PALETTE.length).toBeGreaterThanOrEqual(6);
    for (const c of TILE_PALETTE) expect(contrastRatio(c.background, c.color), c.background).toBeGreaterThanOrEqual(4.5);
  });
  it("measures contrast correctly: black on white is 21 to 1", () => {
    expect(contrastRatio("#000000", "#ffffff")).toBeCloseTo(21, 5);
  });
});

describe("the tile", () => {
  const coin = { account: "cabal2.factory.fairchance.testnet", ticker: "CABAL2" };
  it("shows the picture when the icon fits the coin rule", () => {
    const node = drawCoinTile({ ...coin, icon: validIcon });
    expect(node.tagName).toBe("IMG");
    expect(node.getAttribute("src")).toBe(validIcon);
  });
  it.each([null, undefined, "", "data:image/png;base64,AAAA", "data:image/webp;base64,", "https://evil.example/a.webp", `data:image/webp;base64,${"A".repeat(9000)}`])("shows the letter tile for the icon %s", (icon) => {
    const node = drawCoinTile({ ...coin, icon });
    expect(node.tagName).toBe("DIV");
    expect(node.textContent).toBe("C2");
    expect(node.querySelector("img")).toBeNull();
  });
  it("keeps the small size for a card", () => {
    expect(drawCoinTile({ ...coin, icon: null, small: true }).className).toContain("coin-icon-small");
  });
  it("puts a ticker with markup in as text, never as markup", () => {
    const node = drawCoinTile({ account: "x.factory.fairchance.testnet", ticker: "<b>", icon: null });
    expect(node.children).toHaveLength(0);
    expect(node.textContent).toBe("<B");
  });
});
