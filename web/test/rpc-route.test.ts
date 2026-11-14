// Where the page sends a plain RPC read: its own relay on a hosted origin, the public RPC elsewhere.
import { describe, expect, it, vi } from "vitest";
import { chainReader } from "../src/chain.ts";
import { KNOWN_NETWORKS } from "../src/known-deployments.ts";
import { resolveListLink, resolvePageLink } from "../src/page-link.ts";
import { createRpcRoute, relayUrlFor } from "../src/rpc-route.ts";

const PUBLIC = KNOWN_NETWORKS.testnet.rpcUrl;
const HOSTED = "https://fair-chance.example";
const RELAY = `${HOSTED}/api/rpc`;
const init = { method: "POST", body: "{}" };
const ok = () => new Response(JSON.stringify({ result: {} }));

function fetchBy(answer: (url: string) => Response | Promise<Response>) {
  return vi.fn(async (url: string | URL | Request) => answer(String(url))) as unknown as typeof fetch & ReturnType<typeof vi.fn>;
}
const urls = (f: ReturnType<typeof vi.fn>) => f.mock.calls.map((c) => String(c[0]));

describe("relay URL", () => {
  it("is the page's own /api/rpc on a hosted origin", () => {
    expect(relayUrlFor(HOSTED)).toBe(RELAY);
    expect(relayUrlFor("https://preview-123.vercel.app")).toBe("https://preview-123.vercel.app/api/rpc");
  });

  it.each(["http://localhost:5191", "http://127.0.0.1:5191", "http://[::1]:5191", "http://app.localhost:5191", "null", "file:///x"])(
    "is none on %s",
    (origin) => {
      expect(relayUrlFor(origin)).toBeNull();
    },
  );

  it("is none outside a browser", () => {
    expect(relayUrlFor(null)).toBeNull();
  });
});

describe("RPC route", () => {
  it("sends reads of the known testnet RPC to the relay on a hosted origin", async () => {
    const f = fetchBy(ok);
    const route = createRpcRoute(() => HOSTED);
    await route.post(f, PUBLIC, init);
    expect(urls(f)).toEqual([RELAY]);
    expect(route.relayOn()).toBe(true);
  });

  it("reads the public RPC on the visitor's own machine", async () => {
    const f = fetchBy(ok);
    await createRpcRoute(() => "http://localhost:5191").post(f, PUBLIC, init);
    expect(urls(f)).toEqual([PUBLIC]);
  });

  it("never changes another RPC URL, such as a local sandbox from ?rpc=", async () => {
    const f = fetchBy(ok);
    await createRpcRoute(() => HOSTED).post(f, "http://127.0.0.1:3030", init);
    expect(urls(f)).toEqual(["http://127.0.0.1:3030"]);
  });

  it.each([404, 503])("reads the public RPC for the rest of the session after a relay %i", async (status) => {
    const f = fetchBy((url) => (url === RELAY ? new Response("no", { status }) : ok()));
    const route = createRpcRoute(() => HOSTED);
    expect((await route.post(f, PUBLIC, init)).status).toBe(200);
    await route.post(f, PUBLIC, init);
    expect(urls(f)).toEqual([RELAY, PUBLIC, PUBLIC]);
    expect(route.relayOn()).toBe(false);
  });

  it("reads the public RPC for one read the relay does not allow, and keeps the relay for the next", async () => {
    let first = true;
    const f = fetchBy((url) => {
      if (url === RELAY && first) {
        first = false;
        return new Response(JSON.stringify({ error: "method not allowed" }), { status: 403 });
      }
      return ok();
    });
    const route = createRpcRoute(() => HOSTED);
    await route.post(f, PUBLIC, init);
    await route.post(f, PUBLIC, init);
    expect(urls(f)).toEqual([RELAY, PUBLIC, RELAY]);
  });

  it("reads the public RPC after 3 relay failures in a row", async () => {
    const f = fetchBy((url) => {
      if (url === RELAY) throw new TypeError("Failed to fetch");
      return ok();
    });
    const route = createRpcRoute(() => HOSTED);
    for (let i = 0; i < 3; i++) await expect(route.post(f, PUBLIC, init)).rejects.toThrow("Failed to fetch");
    expect(route.relayOn()).toBe(false);
    expect((await route.post(f, PUBLIC, init)).status).toBe(200);
    expect(urls(f)).toEqual([RELAY, RELAY, RELAY, PUBLIC]);
  });

  it("counts only failures in a row", async () => {
    let n = 0;
    const f = fetchBy(() => (n++ % 2 === 0 ? new Response("busy", { status: 429 }) : ok()));
    const route = createRpcRoute(() => HOSTED);
    for (let i = 0; i < 6; i++) await route.post(f, PUBLIC, init);
    expect(route.relayOn()).toBe(true);
  });

  it("is what the chain reader uses, and its short retry falls back to the public RPC", async () => {
    const f = fetchBy((url) => {
      if (url === RELAY) throw new TypeError("Failed to fetch");
      return new Response(JSON.stringify({ result: { keys: [1, 2] } }));
    });
    const reader = chainReader({ rpcUrl: PUBLIC, coin: "bag.factory.fairchance.testnet" }, f, { sleep: async () => {}, route: createRpcRoute(() => HOSTED) });
    expect(await reader.accessKeyCount("bag.factory.fairchance.testnet")).toBe(2);
    expect(urls(f)).toEqual([RELAY, RELAY, RELAY, PUBLIC]);
  });
});

describe("the host the checks list names", () => {
  it("is the relay on a hosted origin, for reads of the known testnet RPC", () => {
    expect(createRpcRoute(() => HOSTED).readHost(PUBLIC, "test.rpc.fastnear.com")).toBe("fair-chance.example/api/rpc");
  });
  it("is the public RPC on the visitor's own machine", () => {
    expect(createRpcRoute(() => "http://localhost:5191").readHost(PUBLIC, "test.rpc.fastnear.com")).toBe("test.rpc.fastnear.com");
  });
  it("is the RPC of the link for any other RPC URL", () => {
    expect(createRpcRoute(() => HOSTED).readHost("http://127.0.0.1:3030", "127.0.0.1:3030")).toBe("127.0.0.1:3030");
  });
  it("is the public RPC again after the relay is off", async () => {
    const route = createRpcRoute(() => HOSTED);
    await route.post(fetchBy((u) => (u === RELAY ? new Response("", { status: 404 }) : ok())), PUBLIC, init);
    expect(route.readHost(PUBLIC, "test.rpc.fastnear.com")).toBe("test.rpc.fastnear.com");
  });
});

describe("page links stay the same", () => {
  it("still name the public testnet RPC; only the route sends it to the relay", () => {
    expect(resolvePageLink(`${HOSTED}/?coin=bag.factory.fairchance.testnet&network=testnet`).rpcUrl).toBe(PUBLIC);
    const list = resolveListLink(`${HOSTED}/?view=coins&network=testnet`);
    expect(list.kind === "chain" && list.rpcUrl).toBe(PUBLIC);
  });
});
