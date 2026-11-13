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

function listen(handler: (body: string, path: string, res: import("node:http").ServerResponse, search: string) => void): Promise<{ server: Server; origin: string }> {
  const server = createServer((req, res) => {
    for (const [k, v] of Object.entries(cors)) res.setHeader(k, v);
    if (req.method === "OPTIONS") return void res.writeHead(204).end();
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      const at = new URL(req.url ?? "/", "http://localhost");
      handler(body, at.pathname, res, at.search);
    });
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

function runHosted(link: string): Promise<{ code: number; last: string; output: string }> {
  // Async: the fake chain servers live in this process and must keep answering.
  return new Promise((resolve) => {
    const child = spawn("bash", [verifyScript, "hosted", link]);
    let output = "";
    child.stdout.on("data", (c) => (output += c));
    child.stderr.on("data", (c) => (output += c));
    child.on("close", (code) => {
      const lines = output.trim().split("\n");
      resolve({ code: code ?? 1, last: lines[lines.length - 1] ?? "", output });
    });
  });
}

// A host that answers 404 for the page's style file, so the browser logs a console error. Everything else passes through to the local build.
function listenWithoutStyleFile(target: string): Promise<{ server: Server; origin: string }> {
  return listen((_body, path, res) => {
    if (path.endsWith(".css")) return void res.writeHead(404).end();
    void fetch(`${target}${path}`).then(async (r) => res.writeHead(r.status, { "content-type": r.headers.get("content-type") ?? "text/plain" }).end(Buffer.from(await r.arrayBuffer())));
  });
}

// A host that adds one script to the page it serves; the rest passes through to the local build.
function listenWithScript(target: string, script: (originOfSelf: string) => string): Promise<{ server: Server; origin: string }> {
  const served: { origin: string } = { origin: "" };
  return listen((_body, path, res, search) => {
    void fetch(`${target}${path}${search}`).then(async (r) => {
      const type = r.headers.get("content-type") ?? "text/plain";
      const text = type.startsWith("text/html") ? (await r.text()).replace("</body>", `<script>${script(served.origin)}</script></body>`) : Buffer.from(await r.arrayBuffer());
      res.writeHead(r.status, { "content-type": type }).end(text);
    });
  }).then((listening) => {
    served.origin = listening.origin;
    return listening;
  });
}

const page = await buildAndServePage();
const goodRpc = await listen(rpcHandler(true));
const badRpc = await listen(rpcHandler(false));
const fastNear = await listen(fastNearHandler);
const pageWithoutStyleFile = await listenWithoutStyleFile(page.origin);
// A host that redirects every request to another host.
const redirecting = await listen((_body, path, res, search) => void res.writeHead(301, { location: `${pageWithoutStyleFile.origin}${path}${search}` }).end());
// A host whose page sends the browser to another host from script, after it loaded.
const scriptRedirecting = await listenWithScript(page.origin, () => `setTimeout(() => { location.href = ${JSON.stringify(pageWithoutStyleFile.origin + "/")}; }, 800);`);
// A host whose page logs an error that holds a value from the link's query.
const queryLeaking = await listenWithScript(page.origin, () => `setTimeout(() => console.error("bad link secret=" + new URLSearchParams(location.search).get("secret")), 300);`);
// A port with nothing listening, so the browser cannot open the page.
const closed = await listen((_body, _path, res) => void res.end());
const closedOrigin = closed.origin;
await new Promise((done) => closed.server.close(done));
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
  const missing = await runHosted(linkTo(goodRpc.origin, pageWithoutStyleFile.origin));
  console.log(`hosted command, host that answers 404 for its style file: exit ${missing.code}: ${missing.last}`);
  if (missing.code === 0 || !missing.last.startsWith("verify: FAIL (hosted)")) throw new Error("the hosted command did not fail on a 404 from the host");
  // A redirect to another host must fail: the checks would read the other host's errors, not the given host's.
  const redirected = await runHosted(linkTo(goodRpc.origin, redirecting.origin));
  console.log(`hosted command, host that redirects to another host: exit ${redirected.code}: ${redirected.last}`);
  if (redirected.code === 0 || !redirected.last.startsWith("verify: FAIL (hosted)")) throw new Error("the hosted command did not fail on a redirect to another host");
  // A redirect from script after the page loaded must fail too.
  const scripted = await runHosted(linkTo(goodRpc.origin, scriptRedirecting.origin));
  console.log(`hosted command, page that moves to another host from script: exit ${scripted.code}: ${scripted.last}`);
  if (scripted.code === 0 || !scripted.last.startsWith("verify: FAIL (hosted)")) throw new Error("the hosted command did not fail on a script redirect to another host");
  // A console error that holds a value from the link's query must fail and must not print that value.
  const leaking = await runHosted(`${linkTo(goodRpc.origin, queryLeaking.origin)}&secret=SECRETVALUE2`);
  console.log(`hosted command, page error that holds a query value: exit ${leaking.code}: ${leaking.last}`);
  if (leaking.code === 0 || !leaking.last.startsWith("verify: FAIL (hosted)")) throw new Error("the hosted command did not fail on a console error");
  if (leaking.output.includes("SECRETVALUE2")) throw new Error("the hosted command printed a value from the query of the link");
  // A page that cannot open must not print the query of the link, which may hold a key.
  const unreachable = await runHosted(`${closedOrigin}/?coin=moth.factory.sandbox&rpc_key=SECRETVALUE`);
  console.log(`hosted command, page that cannot open: exit ${unreachable.code}: ${unreachable.last}`);
  if (unreachable.code === 0 || !unreachable.last.startsWith("verify: FAIL (hosted)")) throw new Error("the hosted command did not fail on a page that cannot open");
  if (unreachable.output.includes("SECRETVALUE") || unreachable.output.includes("rpc_key")) throw new Error("the hosted command printed the query of the link");
} catch (err) {
  console.error(`check-hosted-page-on-local-build: ${err instanceof Error ? err.message : String(err)}`);
  exitCode = 1;
} finally {
  for (const s of [goodRpc, badRpc, fastNear, pageWithoutStyleFile, redirecting, scriptRedirecting, queryLeaking]) s.server.close();
  await page.close();
}
process.exit(exitCode);
