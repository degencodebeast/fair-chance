// Vercel Node function: a read-only relay for the plain NEAR RPC reads of the page.
// It forwards one allowed JSON-RPC POST body to the URL in FC_RELAY_RPC_URL. That URL holds a key,
// so no response, error or log line ever contains it or any part of it. Plain ESM, no dependencies.

const MAX_BODY_BYTES = 8 * 1024;
const CACHE_TTL_MS = 2_000;
const UPSTREAM_TIMEOUT_MS = 8_000;
const RATE_PER_SECOND = 20;
const RATE_BURST = 40;
const MAX_TRACKED_CLIENTS = 10_000;

const FACTORY = "factory.fairchance.testnet";
const QUERY_ANY_ACCOUNT = new Set(["view_account", "view_access_key_list"]);
const FINALITIES = new Set(["final", "optimistic"]);
const ACCOUNT_ID = /^[a-z0-9._-]{2,64}$/;

const TOP_KEYS = new Set(["jsonrpc", "id", "method", "params"]);
const ACCOUNT_KEYS = new Set(["request_type", "finality", "account_id"]);
const CALL_KEYS = new Set([...ACCOUNT_KEYS, "method_name", "args_base64"]);

/** True when every key of `o` is in `allowed`. */
function onlyKeys(o, allowed) {
  return Object.keys(o).every((k) => allowed.has(k));
}

/** True for the one RPC request shape the page sends: `query` with a fixed finality and no other field. */
export function isAllowed(rpc) {
  if (!rpc || typeof rpc !== "object" || Array.isArray(rpc)) return false;
  if (rpc.method !== "query" || !onlyKeys(rpc, TOP_KEYS)) return false;
  const p = rpc.params;
  if (!p || typeof p !== "object" || Array.isArray(p)) return false;
  if (!FINALITIES.has(p.finality)) return false;
  if (typeof p.account_id !== "string" || !ACCOUNT_ID.test(p.account_id)) return false;
  if (QUERY_ANY_ACCOUNT.has(p.request_type)) return onlyKeys(p, ACCOUNT_KEYS);
  if (p.request_type !== "call_function" || !onlyKeys(p, CALL_KEYS)) return false;
  if (typeof p.method_name !== "string" || typeof p.args_base64 !== "string") return false;
  return p.account_id === FACTORY || p.account_id.endsWith(`.${FACTORY}`);
}

function send(res, status, body, headers = {}) {
  res.statusCode = status;
  res.setHeader("cache-control", "no-store");
  for (const [k, v] of Object.entries(headers)) res.setHeader(k, v);
  if (body === null) {
    res.end();
    return;
  }
  res.setHeader("content-type", "application/json");
  res.end(typeof body === "string" ? body : JSON.stringify(body));
}

/** The request body as text, at most MAX_BODY_BYTES; null when it is too big. */
async function bodyText(req) {
  // Vercel may parse a JSON body before the handler runs.
  if (req.body !== undefined && req.body !== null) {
    if (typeof req.body === "string") return req.body;
    if (req.body instanceof Uint8Array) return Buffer.from(req.body).toString("utf8");
    return JSON.stringify(req.body);
  }
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) return null;
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

function clientIp(req) {
  const forwarded = req.headers["x-forwarded-for"];
  const first = (Array.isArray(forwarded) ? forwarded[0] : forwarded ?? "").split(",")[0].trim();
  return first || req.socket?.remoteAddress || "unknown";
}

/**
 * The relay with its own cache, in-flight map and rate buckets. Tests give their own fetch, env and
 * clock; Vercel uses the default export, one per function instance.
 */
export function createHandler({ fetchFn = (...a) => fetch(...a), env = process.env, now = () => Date.now() } = {}) {
  const cache = new Map();
  const inFlight = new Map();
  const buckets = new Map();

  function overLimit(ip) {
    const t = now();
    let b = buckets.get(ip);
    if (!b) {
      if (buckets.size >= MAX_TRACKED_CLIENTS) buckets.clear();
      b = { tokens: RATE_BURST, at: t };
      buckets.set(ip, b);
    }
    b.tokens = Math.min(RATE_BURST, b.tokens + ((t - b.at) / 1000) * RATE_PER_SECOND);
    b.at = t;
    if (b.tokens < 1) return true;
    b.tokens -= 1;
    return false;
  }

  /** One upstream call. The answer is { status, body } with a fixed body for every failure. */
  async function upstream(url, payload) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), UPSTREAM_TIMEOUT_MS);
    try {
      const resp = await fetchFn(url, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: payload,
        signal: controller.signal,
      });
      if (resp.status === 429 || resp.status >= 500) return { status: resp.status, body: { error: "relay upstream error" } };
      if (resp.status !== 200) return { status: 502, body: { error: "relay upstream error" } };
      const text = await resp.text();
      try {
        JSON.parse(text);
      } catch {
        return { status: 502, body: { error: "relay upstream error" } };
      }
      return { status: 200, body: text };
    } catch {
      if (controller.signal.aborted) return { status: 504, body: { error: "relay upstream timeout" } };
      return { status: 502, body: { error: "relay upstream error" } };
    } finally {
      clearTimeout(timer);
    }
  }

  return async function handler(req, res) {
    try {
      if (req.method === "OPTIONS") return send(res, 204, null, { allow: "POST, OPTIONS" });
      if (req.method !== "POST") return send(res, 405, { error: "method not allowed" }, { allow: "POST, OPTIONS" });
      const type = String(req.headers["content-type"] ?? "").split(";")[0].trim().toLowerCase();
      if (type !== "application/json") return send(res, 415, { error: "content-type must be application/json" });
      if (Number(req.headers["content-length"] ?? 0) > MAX_BODY_BYTES) return send(res, 413, { error: "body too large" });
      if (overLimit(clientIp(req))) return send(res, 429, { error: "too many requests" }, { "retry-after": "1" });

      const text = await bodyText(req);
      if (text === null || Buffer.byteLength(text) > MAX_BODY_BYTES) return send(res, 413, { error: "body too large" });
      let rpc;
      try {
        rpc = JSON.parse(text);
      } catch {
        return send(res, 400, { error: "body is not JSON" });
      }
      if (Array.isArray(rpc)) return send(res, 400, { error: "batch requests are not allowed" });
      if (!isAllowed(rpc)) return send(res, 403, { error: "method not allowed" });

      const url = env.FC_RELAY_RPC_URL;
      if (!url) return send(res, 503, { error: "relay not configured" });

      // The id is left out of the key, so the same read from two pages shares one answer.
      const key = JSON.stringify([rpc.method, rpc.params]);
      const hit = cache.get(key);
      if (hit && hit.until > now()) return send(res, 200, withId(hit.body, rpc.id));
      let pending = inFlight.get(key);
      if (!pending) {
        const payload = JSON.stringify({ jsonrpc: "2.0", id: "fc", method: rpc.method, params: rpc.params });
        pending = upstream(url, payload).finally(() => inFlight.delete(key));
        inFlight.set(key, pending);
      }
      const answer = await pending;
      if (answer.status === 200) {
        cache.set(key, { body: answer.body, until: now() + CACHE_TTL_MS });
        if (cache.size > 1_000) {
          for (const [k, v] of cache) if (v.until <= now()) cache.delete(k);
        }
        return send(res, 200, withId(answer.body, rpc.id));
      }
      return send(res, answer.status, answer.body);
    } catch {
      return send(res, 500, { error: "relay error" });
    }
  };
}

/** The upstream JSON with the caller's id put back. */
function withId(text, id) {
  const body = JSON.parse(text);
  if (body && typeof body === "object" && !Array.isArray(body)) body.id = id ?? null;
  return JSON.stringify(body);
}

export default createHandler();
