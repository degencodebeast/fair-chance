// Self-test of `verify.sh hosted <url>`. It builds the page, serves it on 127.0.0.1 and serves a
// recorded coin state as a fake chain on two more 127.0.0.1 ports. Then it runs the real command
// on that local link. A good chain must give `verify: PASS (hosted)`. A chain that fails every
// read must give `verify: FAIL (hosted)`. No request leaves the machine.
//   node scripts/check-hosted-page-on-local-build.ts
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { fileURLToPath } from "node:url";
import { buildAndServePage } from "./page-server.ts";

const state = JSON.parse(readFileSync(fileURLToPath(new URL("../../testdata/coin-page-states/normal.json", import.meta.url)), "utf8"));
const verifyScript = fileURLToPath(new URL("../../.claude/skills/verify/verify.sh", import.meta.url));
const cors = { "access-control-allow-origin": "*", "access-control-allow-headers": "*" };

function listen(handler: (body: string, path: string, res: import("node:http").ServerResponse) => void): Promise<{ server: Server; origin: string }> {
  const server = createServer((req, res) => {
    for (const [k, v] of Object.entries(cors)) res.setHeader(k, v);
    if (req.method === "OPTIONS") return void res.writeHead(204).end();
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => handler(body, new URL(req.url ?? "/", "http://localhost").pathname, res));
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (address === null || typeof address === "string") return reject(new Error("no port"));
      resolve({ server, origin: `http://127.0.0.1:${address.port}` });
    });
  });
}

function rpcHandler(works: boolean) {
  return (body: string, _path: string, res: import("node:http").ServerResponse) => {
    if (!works) return void res.writeHead(500).end("down");
    const q = JSON.parse(body || "{}").params ?? {};
    let answer: object = {};
    if (q.request_type === "call_function") {
      answer = { result: { result: Array.from(new TextEncoder().encode(JSON.stringify(state.views[q.method_name as string]))) } };
    } else if (q.request_type === "view_account") answer = { result: state.account };
    else if (q.request_type === "view_access_key_list") answer = { result: { keys: [] } };
    res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify({ jsonrpc: "2.0", id: "fc", ...answer }));
  };
}

const fastNearHandler = (_body: string, path: string, res: import("node:http").ServerResponse) => {
  const answer = path === "/v0/account" ? { account_txs: [], resume_token: null } : { transactions: [] };
  res.writeHead(200, { "content-type": "application/json" }).end(JSON.stringify(answer));
};

function runHosted(link: string): Promise<{ code: number; last: string }> {
  // Async: the fake chain servers live in this process and must keep answering.
  return new Promise((resolve) => {
    const child = spawn("bash", [verifyScript, "hosted", link]);
    let output = "";
    child.stdout.on("data", (c) => (output += c));
    child.stderr.on("data", (c) => (output += c));
    child.on("close", (code) => {
      const lines = output.trim().split("\n");
      resolve({ code: code ?? 1, last: lines[lines.length - 1] ?? "" });
    });
  });
}

// A host that answers 404 for the favicon, as a real host may. Everything else passes through to the local build.
function listenWithoutFavicon(target: string): Promise<{ server: Server; origin: string }> {
  return listen((_body, path, res) => {
    if (path === "/favicon.ico") return void res.writeHead(404).end();
    void fetch(`${target}${path}`).then(async (r) => res.writeHead(r.status, { "content-type": r.headers.get("content-type") ?? "text/plain" }).end(Buffer.from(await r.arrayBuffer())));
  });
}

const page = await buildAndServePage();
const goodRpc = await listen(rpcHandler(true));
const badRpc = await listen(rpcHandler(false));
const fastNear = await listen(fastNearHandler);
const pageWithoutFavicon = await listenWithoutFavicon(page.origin);
let exitCode = 0;
try {
  const linkTo = (rpc: string, origin = page.origin) =>
    `${origin}/?coin=moth.factory.sandbox&rpc=${encodeURIComponent(rpc)}&fastnear=${encodeURIComponent(fastNear.origin)}&factory=factory.sandbox&code_hash=${state.account.global_contract_hash}`;
  const good = await runHosted(linkTo(goodRpc.origin));
  console.log(`hosted command, good chain: exit ${good.code}: ${good.last}`);
  if (good.code !== 0 || !good.last.startsWith("verify: PASS (hosted)")) throw new Error("the hosted command did not pass on a good chain");
  const bad = await runHosted(linkTo(badRpc.origin));
  console.log(`hosted command, failing chain: exit ${bad.code}: ${bad.last}`);
  if (bad.code === 0 || !bad.last.startsWith("verify: FAIL (hosted)")) throw new Error("the hosted command did not fail on a chain that fails every read");
  // A real host: a 404 on one of its own files is a console error and must fail the command.
  const missing = await runHosted(linkTo(goodRpc.origin, pageWithoutFavicon.origin));
  console.log(`hosted command, host that answers 404 for its favicon: exit ${missing.code}: ${missing.last}`);
  if (missing.code === 0 || !missing.last.startsWith("verify: FAIL (hosted)")) throw new Error("the hosted command did not fail on a 404 from the host");
} catch (err) {
  console.error(`check-hosted-page-on-local-build: ${err instanceof Error ? err.message : String(err)}`);
  exitCode = 1;
} finally {
  for (const s of [goodRpc, badRpc, fastNear, pageWithoutFavicon]) s.server.close();
  await page.close();
}
process.exit(exitCode);
