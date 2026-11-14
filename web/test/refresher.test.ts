import { describe, expect, it } from "vitest";
import { backoffDelayMs, createRefresher, HttpStatusError, httpStatusError, REFRESH_MS } from "../src/refresher.ts";

function fakeTimers() {
  let now = 0;
  let next = 1;
  const pending = new Map<number, { at: number; fn: () => void }>();
  return {
    setTimeout: (fn: () => void, ms: number) => {
      const id = next++;
      pending.set(id, { at: now + ms, fn });
      return id;
    },
    clearTimeout: (id: unknown) => void pending.delete(id as number),
    /** Moves time forward and runs due timers, letting promises settle between them. */
    async advance(ms: number) {
      const end = now + ms;
      for (;;) {
        const due = [...pending.entries()].filter(([, t]) => t.at <= end).sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        pending.delete(due[0]);
        now = due[1].at;
        due[1].fn();
        for (let i = 0; i < 10; i++) await Promise.resolve();
      }
      now = end;
    },
    count: () => pending.size,
  };
}

describe("backoff delay", () => {
  it("runs 4, 8, 16, 32, then 60 seconds and stays at 60", () => {
    expect([1, 2, 3, 4, 5, 6, 9].map((n) => backoffDelayMs(n, null))).toEqual([4000, 8000, 16000, 32000, 60000, 60000, 60000]);
  });
  it("never waits longer than 60 seconds, however long Retry-After is", () => {
    expect(backoffDelayMs(1, 3_000_000_000)).toBe(60_000);
    expect(backoffDelayMs(2, 61_000)).toBe(60_000);
  });
  it("honors a longer Retry-After and ignores a shorter one", () => {
    expect(backoffDelayMs(1, 30_000)).toBe(30_000);
    expect(backoffDelayMs(3, 1_000)).toBe(16_000);
  });
});

describe("HTTP status errors", () => {
  it("reads Retry-After in seconds from the response", () => {
    const resp = new Response("", { status: 429, headers: { "Retry-After": "12" } });
    const e = httpStatusError("read", resp);
    expect(e).toBeInstanceOf(HttpStatusError);
    expect([e.status, e.retryAfterMs]).toEqual([429, 12_000]);
  });
  it("has no delay when the header is missing or not a number of seconds", () => {
    expect(httpStatusError("x", new Response("", { status: 503 })).retryAfterMs).toBeNull();
    expect(httpStatusError("x", new Response("", { status: 503, headers: { "Retry-After": "soon" } })).retryAfterMs).toBeNull();
  });
});

describe("the refresher", () => {
  function setup(outcomes: Array<"ok" | Error>, hidden = { value: false }) {
    const timers = fakeTimers();
    const calls: number[] = [];
    let i = 0;
    const r = createRefresher({
      run: async () => {
        calls.push(i);
        const o = outcomes[Math.min(i++, outcomes.length - 1)]!;
        if (o !== "ok") throw o;
      },
      timers,
      isHidden: () => hidden.value,
    });
    return { timers, calls, r, hidden };
  }

  it("reads again every interval", async () => {
    const { timers, calls, r } = setup(["ok"]);
    r.start();
    await timers.advance(REFRESH_MS - 1);
    expect(calls).toHaveLength(0);
    await timers.advance(1);
    expect(calls).toHaveLength(1);
    await timers.advance(REFRESH_MS * 2);
    expect(calls).toHaveLength(3);
  });

  it("backs off on 429, then on 5xx, and honors a longer Retry-After", async () => {
    const e429 = new HttpStatusError("x", 429, null);
    const e503 = new HttpStatusError("x", 503, null);
    const e429long = new HttpStatusError("x", 429, 40_000);
    const { timers, calls, r } = setup([e429, e503, e429long, "ok"]);
    r.start();
    await timers.advance(4000); // first read fails
    expect(calls).toHaveLength(1);
    await timers.advance(3999);
    expect(calls).toHaveLength(1);
    await timers.advance(1); // after 4 s: second read fails
    expect(calls).toHaveLength(2);
    await timers.advance(7999);
    expect(calls).toHaveLength(2);
    await timers.advance(1); // after 8 s: third read fails with Retry-After 40 s
    expect(calls).toHaveLength(3);
    await timers.advance(39_999);
    expect(calls).toHaveLength(3);
    await timers.advance(1);
    expect(calls).toHaveLength(4); // success
    await timers.advance(REFRESH_MS); // delay is back to the normal interval
    expect(calls).toHaveLength(5);
  });

  it.each([403, 404, 400])("stops for good on an HTTP %i and leaves no timer", async (status) => {
    const { timers, calls, r } = setup([new HttpStatusError("x", status, null), "ok"]);
    r.start();
    await timers.advance(REFRESH_MS);
    expect(calls).toHaveLength(1);
    expect(timers.count()).toBe(0);
    await timers.advance(REFRESH_MS * 10);
    expect(calls).toHaveLength(1);
  });

  it.each(["Failed to fetch", "NetworkError when attempting to fetch resource.", "Load failed", "Values cannot load: Failed to fetch"])(
    "backs off and tries again when the request got no readable answer (%s)",
    async (message) => {
      const timers = fakeTimers();
      const seen: boolean[] = [];
      let n = 0;
      const r = createRefresher({ run: async () => { if (n++ === 0) throw new TypeError(message); }, timers, isHidden: () => false, onError: (_e, retrying) => seen.push(retrying) });
      r.start();
      await timers.advance(REFRESH_MS);
      expect(seen).toEqual([true]);
      expect(timers.count()).toBe(1);
      await timers.advance(4000);
      expect(n).toBe(2);
    },
  );

  it("stops on an error it does not know, and says so through onError", async () => {
    const timers = fakeTimers();
    const seen: Array<[unknown, boolean]> = [];
    const boom = new Error("odd");
    const r = createRefresher({ run: async () => { throw boom; }, timers, isHidden: () => false, onError: (e, retrying) => seen.push([e, retrying]) });
    r.start();
    await timers.advance(REFRESH_MS * 5);
    expect(seen).toEqual([[boom, false]]);
    expect(timers.count()).toBe(0);
  });

  it("tells onError that a rate limit will be retried", async () => {
    const timers = fakeTimers();
    const seen: boolean[] = [];
    const r = createRefresher({ run: async () => { throw new HttpStatusError("x", 429, null); }, timers, isHidden: () => false, onError: (_e, retrying) => seen.push(retrying) });
    r.start();
    await timers.advance(REFRESH_MS);
    expect(seen).toEqual([true]);
  });

  it("starts again after a stop when asked, with the delays reset", async () => {
    const { timers, calls, r } = setup([new HttpStatusError("x", 403, null), "ok"]);
    r.start();
    await timers.advance(REFRESH_MS);
    expect(timers.count()).toBe(0);
    r.start();
    await timers.advance(REFRESH_MS);
    expect(calls).toHaveLength(2);
  });

  it("stops while the tab is hidden and reads at once when it shows again", async () => {
    const { timers, calls, r, hidden } = setup(["ok"]);
    r.start();
    hidden.value = true;
    await timers.advance(REFRESH_MS * 5);
    expect(calls).toHaveLength(0);
    expect(timers.count()).toBe(0);
    hidden.value = false;
    r.visibilityChanged();
    await timers.advance(0);
    expect(calls).toHaveLength(1);
    await timers.advance(REFRESH_MS);
    expect(calls).toHaveLength(2);
  });

  it("stop cancels the next read", async () => {
    const { timers, calls, r } = setup(["ok"]);
    r.start();
    r.stop();
    await timers.advance(REFRESH_MS * 3);
    expect(calls).toHaveLength(0);
  });

  it("never runs two reads at once", async () => {
    const timers = fakeTimers();
    let active = 0;
    let max = 0;
    let release: () => void = () => {};
    const r = createRefresher({
      run: () => {
        active++;
        max = Math.max(max, active);
        return new Promise<void>((res) => {
          release = () => {
            active--;
            res();
          };
        });
      },
      timers,
      isHidden: () => false,
    });
    r.start();
    await timers.advance(REFRESH_MS);
    r.visibilityChanged();
    await timers.advance(0);
    expect(max).toBe(1);
    release();
  });
});
