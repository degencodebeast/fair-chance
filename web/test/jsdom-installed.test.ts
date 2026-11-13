import { describe, expect, it } from "vitest";

// The DOM tests run in jsdom. If the package is missing, they cannot load, and this test says so by
// name instead of leaving the cause to an error about one of those files.
describe("test tooling", () => {
  it("has jsdom installed", () => {
    expect(() => import.meta.resolve("jsdom")).not.toThrow();
  });
});
