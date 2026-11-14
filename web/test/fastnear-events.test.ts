import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { chainReader, configFromUrl } from "../src/chain.ts";
import { fastNearEvents } from "../src/fastnear-events.ts";
import { HttpStatusError } from "../src/refresher.ts";

const dir = join(import.meta.dirname, "../../testdata/hand-written-fastnear-testnet");
const coin = "walk.factory.sandbox";
const file = (name: string) => readFileSync(join(dir, name));

function fakeFastNear(status = 200): { fetchFn: typeof fetch; calls: string[] } {
  const calls: string[] = [];
  const fetchFn = (async (url: string | URL | Request) => {
    const path = new URL(String(url)).pathname;
    calls.push(path);
    if (status !== 200) return new Response("no", { status });
    const name = path === "/v0/account" ? "account_walking_path.json" : "transactions_walking_path.json";
    return new Response(file(name), { headers: { "content-type": "application/json" } });
  }) as typeof fetch;
  return { fetchFn, calls };
}

describe("FastNEAR event source", () => {
  it("reads the coin events, oldest first, with block and time from the account list", async () => {
    const { fetchFn, calls } = fakeFastNear();
    const { events, cursor } = await fastNearEvents({ baseUrl: "https://fastnear.test", coin, since: 0, fetchFn });
    expect(calls).toEqual(["/v0/account", "/v0/transactions"]);
    expect(events.map((e) => [e.name, e.block, e.timeNs])).toEqual([
      ["fc_buy", 21, "1794538300000000000"],
      ["fc_sell", 27, "1794538400000000000"],
    ]);
    expect(cursor).toBe(28);
  });

  it("leaves out transactions below the start block", async () => {
    const { fetchFn } = fakeFastNear();
    const { events } = await fastNearEvents({ baseUrl: "https://fastnear.test", coin, since: 25, fetchFn });
    expect(events.map((e) => e.name)).toEqual(["fc_sell"]);
  });

  it("tries a 429 3 more times, then passes it on with its Retry-After, so the screen can back off", async () => {
    let calls = 0;
    const waits: number[] = [];
    const fetchFn = (async () => {
      calls++;
      return new Response("slow down", { status: 429, headers: { "Retry-After": "20" } });
    }) as typeof fetch;
    const retry = { sleep: async (ms: number) => void waits.push(ms) };
    const err = await fastNearEvents({ baseUrl: "https://fastnear.test", coin, since: 0, fetchFn, retry }).catch((e: unknown) => e);
    expect(err).toBeInstanceOf(HttpStatusError);
    expect([(err as HttpStatusError).status, (err as HttpStatusError).retryAfterMs]).toEqual([429, 20_000]);
    expect(calls).toBe(4);
    expect(waits).toEqual([8_000, 8_000, 8_000]);
  });

  it("reads again after a request with no readable answer", async () => {
    const { fetchFn: good } = fakeFastNear();
    let failed = 0;
    const fetchFn = (async (url: string | URL | Request, init?: RequestInit) => {
      if (failed++ === 0) throw new TypeError("Failed to fetch");
      return good(url, init);
    }) as typeof fetch;
    const { events } = await fastNearEvents({ baseUrl: "https://fastnear.test", coin, since: 0, fetchFn, retry: { sleep: async () => {} } });
    expect(events).toHaveLength(2);
  });

  it("stops on a 403 and does not retry", async () => {
    const { fetchFn, calls } = fakeFastNear(403);
    await expect(fastNearEvents({ baseUrl: "https://fastnear.test", coin, since: 0, fetchFn })).rejects.toThrow(/403/);
    expect(calls).toHaveLength(1);
  });

  it("is used by the chain reader on a testnet RPC URL, and not on a sandbox URL", async () => {
    expect(configFromUrl("http://x/?coin=a.testnet&network=testnet").fastNearUrl).toBe(
      "https://tx.test.fastnear.com",
    );
    expect(configFromUrl("http://x/?coin=a.sandbox&rpc=http%3A%2F%2F127.0.0.1%3A5").fastNearUrl).toBeUndefined();
    const { fetchFn } = fakeFastNear();
    const reader = chainReader({ coin, rpcUrl: "https://rpc.testnet.example", fastNearUrl: "https://fastnear.test" }, fetchFn);
    const { events } = await reader.events(0);
    expect(events).toHaveLength(2);
  });

  describe("a factory, whose launch event is logged by a callback receipt", () => {
    // FastNEAR marks such a transaction is_event_log:false, so the event-log filter hides it.
    function eventLogBlindFastNear(): { fetchFn: typeof fetch; bodies: Record<string, unknown>[] } {
      const bodies: Record<string, unknown>[] = [];
      const fetchFn = (async (url: string | URL | Request, init?: RequestInit) => {
        const path = new URL(String(url)).pathname;
        if (path === "/v0/transactions") return new Response(file("transactions_walking_path.json"));
        const body = JSON.parse(String(init?.body)) as Record<string, unknown>;
        bodies.push(body);
        const txs = body.is_event_log === true ? [] : JSON.parse(file("account_walking_path.json").toString()).account_txs;
        return new Response(JSON.stringify({ account_txs: txs, resume_token: null }));
      }) as typeof fetch;
      return { fetchFn, bodies };
    }

    it("finds no events with the event-log filter, which is the default for a coin", async () => {
      const { fetchFn } = eventLogBlindFastNear();
      const { events } = await fastNearEvents({ baseUrl: "https://fastnear.test", coin, since: 0, fetchFn });
      expect(events).toEqual([]);
    });

    it("finds the events when the read leaves the filter out, and still asks for successful transactions only", async () => {
      const { fetchFn, bodies } = eventLogBlindFastNear();
      const { events } = await fastNearEvents({ baseUrl: "https://fastnear.test", coin, since: 0, fetchFn, anyLoggedTx: true });
      expect(events.map((e) => e.name)).toEqual(["fc_buy", "fc_sell"]);
      expect(bodies[0]).toMatchObject({ account_id: coin, is_success: true });
      expect(bodies[0]).not.toHaveProperty("is_event_log");
    });

    it("is what the chain reader does when its config asks for it", async () => {
      const { fetchFn } = eventLogBlindFastNear();
      const cfg = { coin, rpcUrl: "https://rpc.testnet.example", fastNearUrl: "https://fastnear.test", anyLoggedTx: true };
      expect((await chainReader(cfg, fetchFn).events(0)).events).toHaveLength(2);
    });
  });
});
