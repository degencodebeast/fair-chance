# RPC relay

`rpc.js` is a Vercel Node function at `/api/rpc`. The hosted page sends its plain NEAR RPC reads to it. The function forwards each allowed read to a keyed RPC provider, so the page does not depend on the public FastNEAR rate limit. Events still come from the FastNEAR transactions API.

To deploy, put this `api/` folder next to the built static files. The function is plain ESM JavaScript for Node 20 or later, with no dependencies.

## Provider URL

The function reads the provider URL from the environment variable `FC_RELAY_RPC_URL`. The URL holds the key, so it never goes in the repository. The builder sets it once for each environment:

```sh
vercel env add FC_RELAY_RPC_URL production
vercel env add FC_RELAY_RPC_URL preview
```

The function never logs, echoes or returns this URL or any part of it. Every upstream failure gets a fixed error text. When the variable is missing, the function answers 503 `{"error":"relay not configured"}`.

## Allow-list

The function forwards only JSON-RPC method `query` with `finality` set to `final` or `optimistic` (no `block_id`):

- `request_type` `view_account` for any account.
- `request_type` `view_access_key_list` for any account.
- `request_type` `call_function` only for `factory.fairchance.testnet` and accounts that end with `.factory.fairchance.testnet`.

The page sends no other RPC method to the relay on testnet: events come from the FastNEAR transactions API, and the wallet uses its own RPC. Every other request gets 403 `{"error":"method not allowed"}`.

The function also refuses:

- A batch request (400).
- A body over 8 KB (413).
- A body that is not JSON (400), or a content type other than `application/json` (415).
- Any HTTP method other than `POST` (405). `OPTIONS` answers 204.

## Cache and limits

- Cache: in memory, per function instance, keyed by method and params. An answer stays 2 seconds. Only a 200 answer goes in the cache.
- Identical reads in flight share one upstream call.
- Client limit: a token bucket per first `x-forwarded-for` address, 20 requests a second with a burst of 40. Over the limit the answer is 429 with `Retry-After: 1`.
- Upstream timeout: 8 seconds, then 504 `{"error":"relay upstream timeout"}`.
- An upstream 429 or 5xx gives the same status with `{"error":"relay upstream error"}`. Any other upstream failure gives 502 with the same body.

## Page side

On a hosted origin the page reads the known testnet RPC through `/api/rpc`. On `localhost`, `127.0.0.1`, `[::1]` and `*.localhost` it reads `https://test.rpc.fastnear.com` directly. When the relay answers 404 or 503, or fails 3 times in a row, the page reads the public FastNEAR RPC for the rest of the page session.

Tests: `web/test/relay.test.ts`.
