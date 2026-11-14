import { describe, expect, it } from "vitest";
import { age, compactTokens, compactTokensRoundedUp, nearAmount, percentOfBps, timeWithHint } from "../src/format.ts";

const MIN = 60n * 1_000_000_000n;
const HOUR = 60n * MIN;
// 2026-11-13 14:24:00 UTC
const NOW = 1794579840000000000n;

describe("amounts", () => {
  it("writes tokens with K, M and B", () => {
    expect(compactTokens("10152544552210749144063374", 18)).toBe("10.1M");
    expect(compactTokens("100000000000000000000000", 18)).toBe("100K");
    expect(compactTokens("1000000000000000000000000000", 18)).toBe("1.0B");
    expect(compactTokens("0", 18)).toBe("0");
    expect(compactTokens("1500000000000000000", 18)).toBe("1.5");
  });
  it("writes tokens rounded up at the shown precision, so the text is at or above the amount", () => {
    expect(compactTokensRoundedUp("6040000000000000000000000", 18)).toBe("6.1M");
    expect(compactTokensRoundedUp("6000000000000000000000000", 18)).toBe("6.0M");
    expect(compactTokensRoundedUp("100000000000000000000001", 18)).toBe("100.1K");
    expect(compactTokensRoundedUp("100000000000000000000000", 18)).toBe("100K");
    expect(compactTokensRoundedUp("999950000000000000000000", 18)).toBe("1.0M");
    expect(compactTokensRoundedUp("1", 18)).toBe("0.1");
    expect(compactTokensRoundedUp("0", 18)).toBe("0");
  });
  it("writes NEAR cut to two places, or more for small amounts", () => {
    expect(nearAmount("40470000000000000000000000")).toBe("40.47");
    expect(nearAmount("2130000000000000000000000")).toBe("2.13");
    expect(nearAmount("0")).toBe("0.00");
    expect(nearAmount("502487562189054726368159")).toBe("0.50");
    expect(nearAmount("5024875621890547263681", 4)).toBe("0.0050");
  });
  it("writes basis points as a percent", () => {
    expect(percentOfBps(390)).toBe("3.9%");
    expect(percentOfBps(10)).toBe("0.1%");
    expect(percentOfBps(0)).toBe("0.0%");
  });
});

describe("time", () => {
  it("writes local 24 h time with a relative hint", () => {
    expect(timeWithHint(NOW + 38n * MIN, { nowNs: NOW, timeZone: "UTC" })).toBe("15:02, in 38 min");
    expect(timeWithHint(NOW - 3n * MIN, { nowNs: NOW, timeZone: "UTC" })).toBe("14:21, 3 min ago");
    expect(timeWithHint(NOW + 22n * HOUR + 38n * MIN, { nowNs: NOW, timeZone: "UTC" })).toBe(
      "13:02 tomorrow, in 22 h 38 min",
    );
  });
  it("follows the time zone it is given", () => {
    expect(timeWithHint(NOW, { nowNs: NOW, timeZone: "Africa/Lagos" })).toBe("15:24, now");
  });
  it("writes an age", () => {
    expect(age(82n * MIN)).toBe("1 h 22 min");
    expect(age(40n * 1_000_000_000n)).toBe("40 s");
    expect(age(50n * HOUR)).toBe("2 days 2 h");
  });
});

describe("exactNear", () => {
  it("writes the whole amount of NEAR without trailing zeros", async () => {
    const { exactNear } = await import("../src/format.ts");
    expect(exactNear("1250000000000000000000")).toBe("0.00125");
    expect(exactNear("1000000000000000000000000")).toBe("1");
    expect(exactNear("1500000000000000000000000")).toBe("1.5");
    expect(exactNear("0")).toBe("0");
  });
});

describe("zone label", () => {
  it("names the offset from UTC, and plain UTC for none", async () => {
    const { zoneLabel, clockTimeWithZone } = await import("../src/format.ts");
    const ns = BigInt(Date.UTC(2026, 10, 14, 10, 11)) * 1_000_000n;
    expect(zoneLabel(ns, { timeZone: "UTC" })).toBe("UTC");
    expect(clockTimeWithZone(ns, { timeZone: "Africa/Lagos" })).toBe("11:11 UTC+1");
    expect(zoneLabel(ns, { timeZone: "Asia/Kolkata" })).toBe("UTC+5:30");
  });
});
