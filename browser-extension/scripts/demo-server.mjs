/**
 * Serves demo/ on http://localhost:5179 (content scripts do not run on file:// pages by
 * default). Usage: node scripts/demo-server.mjs [port]
 */
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, resolve, sep } from "node:path";

const dir = resolve(import.meta.dirname, "../demo");
const types = { ".html": "text/html; charset=utf-8", ".js": "text/javascript", ".css": "text/css" };

export function serveDemo(port = 5179) {
  const server = createServer(async (req, res) => {
    const path = new URL(req.url ?? "/", "http://localhost").pathname;
    const file = resolve(dir, `.${path === "/" ? "/index.html" : path}`);
    if (!file.startsWith(dir + sep)) {
      res.writeHead(403).end();
      return;
    }
    try {
      const body = await readFile(file);
      res.writeHead(200, { "Content-Type": types[extname(file)] ?? "application/octet-stream" }).end(body);
    } catch {
      res.writeHead(404).end("not found");
    }
  });
  return new Promise((ok) => server.listen(port, "127.0.0.1", () => ok(server)));
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const port = Number(process.argv[2] ?? 5179);
  await serveDemo(port);
  console.log(`Forge Companion demo: http://localhost:${port}/`);
}
