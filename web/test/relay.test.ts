// @vitest-environment node
// The server relay for plain RPC reads (web/api/rpc.js), on a fake upstream. No network.
import { describe, expect, it, vi } from "vitest";
// @ts-expect-error: the relay is plain JavaScript with no type file, so Vercel builds it as is.
import { createHandler, isAllowed } from "../api/rpc.js";

const FAKE_KEY = "fakekey-0123456789abcdef";
const FAKE_URL = `https://rpc.provider.invalid/v1/${FAKE_KEY}`;
const ENV = { FC_RELAY_RPC_URL: FAKE_URL };

type Answer = { status: number; headers: Record<string, string>; text: string };

function request(body: unknown, over: { method?: string; headers?: Record<string, string>; raw?: string } = {}) {
  const text = over.raw ?? (body === undefined ? "" : JSON.stringify(body));
  return {
    method: over.method ?? "POST",
    headers: { "content-type": "application/json", "x-forwarded-for": "203.0.113.7, 10.0.0.1", ...over.headers },
    socket: {},
    async *[Symbol.asyncIterator]() {
      if (text !== "") yield Buffer.from(text);
    },
  };
}

function response(): { res: object; done: Promise<Answer> } {
  let finish!: (a: Answer) => void;
  const done = new Promise<Answer>((r) => (finish = r));
  const headers: Record<string, string> = {};
  const res = {
    statusCode: 200,
    setHeader(k: string, v: string) {
      headers[k.toLowerCase()] = v;
    },
    end(text?: string) {
      finish({ status: this.statusCode, headers, text: text ?? "" });
    },
  };
  return { res, done };
}

type Handler = (req: unknown, res: unknown) => Promise<void>;

async function call(handler: Handler, body: unknown, over: Parameters<typeof request>[1] = {}): Promise<Answer> {
  const { res, done } = response();
  await handler(request(body, over), res);
  return done;
}

const view = (account_id: string, method_name = "fc_rules") => ({
  jsonrpc: "2.0",
  id: "fc",
  method: "query",
  params: { request_type: "call_function", finality: "final", account_id, method_name, args_base64: "e30=" },
});
const viewAccount = (account_id: string) => ({ jsonrpc: "2.0", id: "fc", method: "query", params: { request_type: "view_account", finality: "final", account_id } });

function upstream(reply: () => Response | Promise<Response> = () => new Response(JSON.stringify({ jsonrpc: "2.0", id: "fc", result: { ok: 1 } }))) {
  return vi.fn(async (_url: string, _init: RequestInit) => reply());
}

function relay(fetchFn = upstream(), env: Record<string, string | undefined> = ENV, now = () => 1_000) {
  return { handler: createHandler({ fetchFn, env, now }) as Handler, fetchFn };
}

/** Every answer of these tests: none may name the keyed URL or its key. */
function expectNoSecret(a: Answer): void {
  const all = a.text + JSON.stringify(a.headers);
  expect(all).not.toContain(FAKE_KEY);
  expect(all).not.toContain("provider.invalid");
}

describe("relay allow-list", () => {
  it("passes the reads the page sends", () => {
    expect(isAllowed(viewAccount("anyone.testnet"))).toBe(true);
    expect(isAllowed({ method: "query", params: { request_type: "view_access_key_list", finality: "final", account_id: "anyone.testnet" } })).toBe(true);
    expect(isAllowed(view("factory.fairchance.testnet"))).toBe(true);
    expect(isAllowed(view("bag.factory.fairchance.testnet"))).toBe(true);
    expect(isAllowed({ ...viewAccount("a.testnet"), params: { ...viewAccount("a.testnet").params, finality: "optimistic" } })).toBe(true);
  });

  it.each([
    ["a view of another contract", view("wrap.testnet")],
    ["a look-alike factory", view("evilfactory.fairchance.testnet")],
    ["a look-alike suffix", view("bag.factory.fairchance.testnet.evil.testnet")],
    ["a broadcast", { method: "broadcast_tx_commit", params: ["AAAA"] }],
    ["a block read", { method: "block", params: { finality: "final" } }],
    ["a tx status", { method: "EXPERIMENTAL_tx_status", params: { tx_hash: "x", sender_account_id: "a.testnet" } }],
    ["a state dump", { method: "query", params: { request_type: "view_state", finality: "final", account_id: "a.testnet", prefix_base64: "" } }],
    ["a param it does not know", { ...viewAccount("a.testnet"), params: { ...viewAccount("a.testnet").params, sync_checkpoint: "genesis" } }],
    ["an extra param on a view call", { ...view("bag.factory.fairchance.testnet"), params: { ...view("bag.factory.fairchance.testnet").params, sync_checkpoint: "genesis" } }],
    ["a view call with no method name", { ...view("bag.factory.fairchance.testnet"), params: { request_type: "call_function", finality: "final", account_id: "bag.factory.fairchance.testnet", args_base64: "e30=" } }],
    ["a top-level field it does not know", { ...viewAccount("a.testnet"), extra: 1 }],
    ["a read at an old block", { method: "query", params: { request_type: "view_account", block_id: 1, account_id: "a.testnet" } }],
  ])("refuses %s", (_name, body) => {
    expect(isAllowed(body)).toBe(false);
  });

  it("answers 403 for a method that is not allowed and never calls upstream", async () => {
    const { handler, fetchFn } = relay();
    const a = await call(handler, view("wrap.testnet"));
    expect([a.status, JSON.parse(a.text)]).toEqual([403, { error: "method not allowed" }]);
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it("refuses a batch, a big body, a body that is not JSON, other HTTP methods and other content types", async () => {
    const { handler, fetchFn } = relay();
    expect((await call(handler, [viewAccount("a.testnet")])).status).toBe(400);
    expect((await call(handler, { ...viewAccount("a.testnet"), pad: "x".repeat(9000) })).status).toBe(413);
    expect((await call(handler, undefined, { raw: "{nope" })).status).toBe(400);
    expect((await call(handler, undefined, { method: "GET" })).status).toBe(405);
    expect((await call(handler, viewAccount("a.testnet"), { headers: { "content-type": "text/plain" } })).status).toBe(415);
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it("answers OPTIONS with 204", async () => {
    const { handler } = relay();
    expect((await call(handler, undefined, { method: "OPTIONS" })).status).toBe(204);
  });

  it("reads a body Vercel already parsed", async () => {
    const { handler, fetchFn } = relay();
    const { res, done } = response();
    await handler({ ...request(undefined), body: viewAccount("a.testnet") }, res);
    expect((await done).status).toBe(200);
    expect(fetchFn).toHaveBeenCalledTimes(1);
  });
});

describe("relay forwarding", () => {
  it("forwards an allowed read to the keyed URL and passes the JSON through with the caller's id", async () => {
    const { handler, fetchFn } = relay();
    const a = await call(handler, { ...viewAccount("a.testnet"), id: 7 });
    expect(a.status).toBe(200);
    expect(JSON.parse(a.text)).toEqual({ jsonrpc: "2.0", id: 7, result: { ok: 1 } });
    expect(fetchFn.mock.calls[0]![0]).toBe(FAKE_URL);
    expect(JSON.parse(String(fetchFn.mock.calls[0]![1].body))).toMatchObject({ method: "query", params: { account_id: "a.testnet" } });
    expectNoSecret(a);
  });

  it("answers 503 when the env var is missing", async () => {
    const { handler, fetchFn } = relay(upstream(), {});
    const a = await call(handler, viewAccount("a.testnet"));
    expect([a.status, JSON.parse(a.text)]).toEqual([503, { error: "relay not configured" }]);
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it.each([429, 500, 503])("passes an upstream %i on with a fixed body", async (status) => {
    const { handler } = relay(upstream(() => new Response(`upstream says ${FAKE_URL}`, { status })));
    const a = await call(handler, viewAccount("a.testnet"));
    expect([a.status, JSON.parse(a.text)]).toEqual([status, { error: "relay upstream error" }]);
    expectNoSecret(a);
  });

  it("never names the keyed URL when upstream throws an error that contains it", async () => {
    const { handler } = relay(upstream(() => { throw new TypeError(`fetch failed for ${FAKE_URL}`); }));
    const log = vi.spyOn(console, "error").mockImplementation(() => {});
    const warn = vi.spyOn(console, "log").mockImplementation(() => {});
    const a = await call(handler, viewAccount("a.testnet"));
    expect([a.status, JSON.parse(a.text)]).toEqual([502, { error: "relay upstream error" }]);
    expectNoSecret(a);
    expect(log).not.toHaveBeenCalled();
    expect(warn).not.toHaveBeenCalled();
    log.mockRestore();
    warn.mockRestore();
  });

  it("answers 502 for an upstream 200 that is not JSON", async () => {
    const { handler } = relay(upstream(() => new Response(`<html>${FAKE_KEY}</html>`)));
    const a = await call(handler, viewAccount("a.testnet"));
    expect(a.status).toBe(502);
    expectNoSecret(a);
  });

  it("gives up on upstream after 8 seconds", async () => {
    vi.useFakeTimers();
    try {
      const { handler } = relay(
        vi.fn((_url: string, init: RequestInit) => new Promise<Response>((_r, reject) => init.signal!.addEventListener("abort", () => reject(new Error(`aborted ${FAKE_URL}`))))),
      );
      const answer = call(handler, viewAccount("a.testnet"));
      await vi.advanceTimersByTimeAsync(7_999);
      let settled = false;
      void answer.then(() => (settled = true));
      await vi.advanceTimersByTimeAsync(0);
      expect(settled).toBe(false);
      await vi.advanceTimersByTimeAsync(1);
      const a = await answer;
      expect([a.status, JSON.parse(a.text)]).toEqual([504, { error: "relay upstream timeout" }]);
      expectNoSecret(a);
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("relay cache and limits", () => {
  it("answers the same read from its cache for 2 seconds", async () => {
    let t = 1_000;
    const { handler, fetchFn } = relay(upstream(), ENV, () => t);
    await call(handler, viewAccount("a.testnet"));
    t += 1_999;
    expect((await call(handler, { ...viewAccount("a.testnet"), id: "other" })).status).toBe(200);
    expect(fetchFn).toHaveBeenCalledTimes(1);
    t += 1;
    await call(handler, viewAccount("a.testnet"));
    expect(fetchFn).toHaveBeenCalledTimes(2);
    await call(handler, viewAccount("b.testnet"));
    expect(fetchFn).toHaveBeenCalledTimes(3);
  });

  it("does not cache a failed upstream answer", async () => {
    let n = 0;
    const { handler, fetchFn } = relay(upstream(() => (n++ === 0 ? new Response("busy", { status: 429 }) : new Response(JSON.stringify({ result: {} })))));
    expect((await call(handler, viewAccount("a.testnet"))).status).toBe(429);
    expect((await call(handler, viewAccount("a.testnet"))).status).toBe(200);
    expect(fetchFn).toHaveBeenCalledTimes(2);
  });

  it("sends one upstream call for identical reads in flight", async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const { handler, fetchFn } = relay(upstream(async () => {
      await gate;
      return new Response(JSON.stringify({ result: { n: 1 } }));
    }));
    const answers = Promise.all([1, 2, 3].map((id) => call(handler, { ...viewAccount("a.testnet"), id })));
    await new Promise((r) => setTimeout(r, 5));
    release();
    const all = await answers;
    expect(all.map((a) => [a.status, JSON.parse(a.text).id])).toEqual([[200, 1], [200, 2], [200, 3]]);
    expect(fetchFn).toHaveBeenCalledTimes(1);
  });

  it("allows a burst of 40 per client, then answers 429 with Retry-After 1, and refills 20 a second", async () => {
    let t = 1_000;
    const { handler } = relay(upstream(), ENV, () => t);
    const statuses: number[] = [];
    for (let i = 0; i < 41; i++) statuses.push((await call(handler, viewAccount("a.testnet"))).status);
    expect(statuses.slice(0, 40).every((s) => s === 200)).toBe(true);
    const over = await call(handler, viewAccount("a.testnet"));
    expect([over.status, over.headers["retry-after"]]).toEqual([429, "1"]);
    // Another client has its own bucket.
    expect((await call(handler, viewAccount("a.testnet"), { headers: { "x-forwarded-for": "198.51.100.1" } })).status).toBe(200);
    t += 100; // 2 more tokens
    expect((await call(handler, viewAccount("a.testnet"))).status).toBe(200);
  });
});
