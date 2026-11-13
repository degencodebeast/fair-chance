import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { chainReader, configFromUrl } from "../src/chain.ts";
import { fastNearEvents } from "../src/fastnear-events.ts";

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

  it("stops on a 403 and does not retry", async () => {
    const { fetchFn, calls } = fakeFastNear(403);
    await expect(fastNearEvents({ baseUrl: "https://fastnear.test", coin, since: 0, fetchFn })).rejects.toThrow(/403/);
    expect(calls).toHaveLength(1);
  });

  it("is used by the chain reader on a testnet RPC URL, and not on a sandbox URL", async () => {
    expect(configFromUrl("http://x/?coin=a.testnet&rpc=https%3A%2F%2Frpc.testnet.example").fastNearUrl).toBe(
      "https://tx.test.fastnear.com",
    );
    expect(configFromUrl("http://x/?coin=a.sandbox&rpc=http%3A%2F%2F127.0.0.1%3A5").fastNearUrl).toBeUndefined();
    const { fetchFn } = fakeFastNear();
    const reader = chainReader({ coin, rpcUrl: "https://rpc.testnet.example", fastNearUrl: "https://fastnear.test" }, fetchFn);
    const { events } = await reader.events(0);
    expect(events).toHaveLength(2);
  });
});
