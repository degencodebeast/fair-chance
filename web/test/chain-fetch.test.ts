// The shared read limit and the short retry of one chain read.
import { afterEach, describe, expect, it, vi } from "vitest";
import { createReadLimiter, isRetryableRead, readWithRetry, retryDelayMs } from "../src/chain-fetch.ts";
import { HttpStatusError } from "../src/refresher.ts";

afterEach(() => {
  vi.useRealTimers();
});

describe("the read limiter", () => {
  it("runs at most 4 reads at a time and queues the rest in order", async () => {
    const limiter = createReadLimiter(4);
    let inFlight = 0;
    let most = 0;
    const started: number[] = [];
    const releases: Array<() => void> = [];
    const reads = Array.from({ length: 10 }, (_, i) =>
      limiter.run(async () => {
        started.push(i);
        inFlight++;
        most = Math.max(most, inFlight);
        await new Promise<void>((r) => releases.push(r));
        inFlight--;
        return i;
      }),
    );
    await Promise.resolve();
    expect(started).toEqual([0, 1, 2, 3]);
    while (releases.length > 0) {
      releases.shift()!();
      for (let i = 0; i < 5; i++) await Promise.resolve();
    }
    expect(await Promise.all(reads)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    expect(most).toBe(4);
    expect(started).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
  });

  it("frees the slot when a read fails", async () => {
    const limiter = createReadLimiter(1);
    await expect(limiter.run(async () => { throw new Error("no"); })).rejects.toThrow("no");
    expect(await limiter.run(async () => "next")).toBe("next");
  });
});

describe("which reads are tried again", () => {
  it.each([
    new TypeError("Failed to fetch"),
    new TypeError("NetworkError when attempting to fetch resource."),
    new TypeError("Load failed"),
    new HttpStatusError("x", 429, null),
    new HttpStatusError("x", 500, null),
    new HttpStatusError("x", 503, null),
  ])("tries again after %s", (e) => {
    expect(isRetryableRead(e)).toBe(true);
  });

  it.each([new HttpStatusError("x", 403, null), new HttpStatusError("x", 404, null), new HttpStatusError("x", 400, null), new Error("odd")])(
    "does not try again after %s",
    (e) => {
      expect(isRetryableRead(e)).toBe(false);
    },
  );

  it("waits 1, 2 and 4 seconds, and honors Retry-After up to 8 seconds", () => {
    expect([1, 2, 3].map((n) => retryDelayMs(n, null))).toEqual([1000, 2000, 4000]);
    expect(retryDelayMs(1, 5_000)).toBe(5_000);
    expect(retryDelayMs(1, 30_000)).toBe(8_000);
    expect(retryDelayMs(3, 1_000)).toBe(4_000);
  });
});

describe("one read with a short retry", () => {
  it("tries up to 3 more times after 1, 2 and 4 seconds, then throws the last error unchanged", async () => {
    vi.useFakeTimers();
    const last = new TypeError("Failed to fetch");
    const at: number[] = [];
    const start = Date.now();
    const read = readWithRetry(async () => {
      at.push(Date.now() - start);
      throw at.length === 4 ? last : new TypeError("Failed to fetch");
    });
    const caught = read.catch((e: unknown) => e);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(await caught).toBe(last);
    expect(at).toEqual([0, 1000, 3000, 7000]);
  });

  it("gives the answer of the first try that works", async () => {
    vi.useFakeTimers();
    let n = 0;
    const read = readWithRetry(async () => {
      if (n++ < 2) throw new HttpStatusError("busy", 429, null);
      return "ok";
    });
    await vi.advanceTimersByTimeAsync(3000);
    expect(await read).toBe("ok");
    expect(n).toBe(3);
  });

  it("waits as long as Retry-After says, up to 8 seconds", async () => {
    vi.useFakeTimers();
    let n = 0;
    const read = readWithRetry(async () => {
      if (n++ === 0) throw new HttpStatusError("busy", 429, 6_000);
      return "ok";
    });
    await vi.advanceTimersByTimeAsync(5_999);
    expect(n).toBe(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(await read).toBe("ok");
  });

  it("does not try again after an answer it does not know", async () => {
    let n = 0;
    const gone = new HttpStatusError("gone", 403, null);
    await expect(readWithRetry(async () => { n++; throw gone; })).rejects.toBe(gone);
    expect(n).toBe(1);
  });

  it("holds no read slot while it waits to try again", async () => {
    vi.useFakeTimers();
    const limiter = createReadLimiter(1);
    let n = 0;
    const flaky = readWithRetry(async () => {
      if (n++ === 0) throw new TypeError("Failed to fetch");
      return "flaky";
    }, { limiter });
    await vi.advanceTimersByTimeAsync(0);
    // The other read runs while the first one waits its 1 second.
    expect(await readWithRetry(async () => "other", { limiter })).toBe("other");
    await vi.advanceTimersByTimeAsync(1000);
    expect(await flaky).toBe("flaky");
  });
});
