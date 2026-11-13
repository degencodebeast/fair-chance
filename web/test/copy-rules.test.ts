import { describe, expect, it } from "vitest";
import { copyViolations } from "./copy-rules.ts";

describe("copy rules for the early window time", () => {
  it("refuses 17 seconds and 18 seconds", () => {
    expect(copyViolations("The early window is 17 seconds long.")).toContain('banned word "17 seconds"');
    expect(copyViolations("It lasts 18 seconds.")).toContain('banned word "18 seconds"');
  });

  it("refuses 'about 10 seconds' without the measured block time wording", () => {
    expect(copyViolations("Blocks 0 to 17, about 10 seconds.")).toContain("early window time without the measured mainnet block time");
  });

  it("accepts the wording with the measured mainnet block time", () => {
    expect(copyViolations("Blocks 0 to 17, about 10 seconds at the measured mainnet block time.")).toEqual([]);
  });
});
