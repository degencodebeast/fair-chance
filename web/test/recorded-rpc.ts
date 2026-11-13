// Serves recorded NEAR JSON-RPC responses as a fetch function, by the same file rules as the
// Go watcher's fake: block_<height>.json, chunk_<hash>.json, tx_<hash>.json, query_<method>.json.
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

export const RECORDED = join(import.meta.dirname, "../../testdata/recorded-rpc-walking-path");

export function recordedFetch(dir: string, head: number): typeof fetch {
  return (async (_url: string | URL | Request, init?: RequestInit) => {
    const req = JSON.parse(String(init?.body));
    const p = req.params ?? {};
    let name: string;
    switch (req.method) {
      case "block": {
        const height = p.block_id ?? head;
        name = `block_${height}.json`;
        if (!existsSync(join(dir, name))) {
          return Response.json({
            jsonrpc: "2.0",
            id: "fc",
            error: {
              name: "HANDLER_ERROR",
              cause: { info: {}, name: "UNKNOWN_BLOCK" },
              code: -32000,
              message: "Server error",
              data: `DB Not Found Error: BLOCK HEIGHT: ${height} \n Cause: Unknown`,
            },
          });
        }
        break;
      }
      case "chunk":
        name = `chunk_${p.chunk_id}.json`;
        break;
      case "EXPERIMENTAL_tx_status":
        name = `tx_${p.tx_hash}.json`;
        break;
      case "query":
        name = `query_${p.method_name}.json`;
        break;
      default:
        return new Response(`method not recorded: ${req.method}`, { status: 404 });
    }
    const file = join(dir, name);
    if (!existsSync(file)) return new Response(`not recorded: ${name}`, { status: 404 });
    return new Response(readFileSync(file), { headers: { "content-type": "application/json" } });
  }) as typeof fetch;
}
