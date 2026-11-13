import { describe, expect, it } from "vitest";
import { screenFor, screenLink } from "../src/screen-route.ts";

describe("launch screen route", () => {
  it("opens the launch form for view=launch", () => {
    expect(screenFor("http://localhost/?view=launch")).toBe("launch");
  });
  it("links to it and keeps the source parameters", () => {
    expect(screenLink("http://localhost/?network=testnet", "launch")).toBe("?view=launch&network=testnet");
  });
});
