// Builds the coin page with Vite into its own temporary folder and serves it on a free local port.
// The browser scripts share it. Close with `close()`; it removes the temporary folder.
import { createServer } from "node:http";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const webRoot = fileURLToPath(new URL("..", import.meta.url));

const contentTypes: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".svg": "image/svg+xml",
};

export async function buildAndServePage(): Promise<{ origin: string; close: () => Promise<void> }> {
  const scratch = await mkdtemp(join(tmpdir(), "fair-chance-page-"));
  const folder = join(scratch, "page");
  await build({
    root: webRoot,
    configFile: false,
    logLevel: "silent",
    cacheDir: join(scratch, "vite-cache"),
    build: { outDir: folder, emptyOutDir: true },
  });
  const server = createServer(async (req, res) => {
    const path = new URL(req.url ?? "/", "http://localhost").pathname;
    const relative = normalize(path === "/" ? "/index.html" : path);
    if (relative.includes("..")) {
      res.writeHead(400).end();
      return;
    }
    // Browsers ask for a favicon on their own; the page has none, so answer "no content", not 404.
    if (path === "/favicon.ico") {
      res.writeHead(204).end();
      return;
    }
    try {
      const body = await readFile(join(folder, relative));
      res.writeHead(200, { "content-type": contentTypes[extname(relative)] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  const origin = await new Promise<string>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (address === null || typeof address === "string") return reject(new Error("no port"));
      resolve(`http://127.0.0.1:${address.port}`);
    });
  });
  return {
    origin,
    close: async () => {
      server.close();
      await rm(scratch, { recursive: true, force: true });
    },
  };
}
