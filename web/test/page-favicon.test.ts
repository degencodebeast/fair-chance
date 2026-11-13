import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("page icon", () => {
  it("names its own icon, so no browser asks the host for /favicon.ico", () => {
    const html = readFileSync(new URL("../index.html", import.meta.url), "utf8");
    expect(html).toMatch(/<link[^>]+rel="icon"[^>]+href="data:/);
  });
});
