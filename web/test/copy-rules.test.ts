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

  it("refuses a second bare 'about 10 seconds' after a good one", () => {
    const text = "Blocks 0 to 17, about 10 seconds at the measured mainnet block time. Later, about 10 seconds.";
    expect(copyViolations(text)).toContain("early window time without the measured mainnet block time");
  });

  it("refuses 'about 10 seconds' written with a no-break space or two spaces", () => {
    expect(copyViolations("Blocks 0 to 17, about\u00a010\u00a0seconds.")).toContain("early window time without the measured mainnet block time");
    expect(copyViolations("Blocks 0 to 17, about  10  seconds.")).toContain("early window time without the measured mainnet block time");
  });

  it("refuses 17-second and 18-second, and the numbers with odd spaces", () => {
    expect(copyViolations("A 17-second window.")).toContain('banned word "17-second"');
    expect(copyViolations("An 18-second window.")).toContain('banned word "18-second"');
    expect(copyViolations("It lasts 17\u00a0seconds.")).toContain('banned word "17 seconds"');
    expect(copyViolations("It lasts 18  seconds.")).toContain('banned word "18 seconds"');
  });
});
