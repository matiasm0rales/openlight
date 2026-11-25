// Servidor estático para desarrollo y pruebas, sin dependencias. Manda las mismas cabeceras que
// define vercel.json, así una política (CSP) demasiado estricta se nota antes de publicar.
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, normalize, sep } from "node:path";
import { fileURLToPath } from "node:url";

const web = fileURLToPath(new URL("..", import.meta.url));
const root = join(web, "public");
const port = Number(process.env.PORT) || 4180;

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".woff2": "font/woff2",
  ".txt": "text/plain; charset=utf-8",
};

// Solo se aplican las reglas generales ("/(.*)"); las de caché por carpeta no importan en local.
const config = JSON.parse(await readFile(join(web, "vercel.json"), "utf8").catch(() => "{}"));
const headers = Object.fromEntries(
  (config.headers ?? []).filter((rule) => rule.source === "/(.*)").flatMap((rule) => rule.headers.map((h) => [h.key, h.value])),
);

createServer(async (request, response) => {
  try {
    const path = normalize(decodeURIComponent(new URL(request.url, "http://localhost").pathname));
    const file = join(root, path.endsWith(sep) || path.endsWith("/") ? join(path, "index.html") : path);
    if (!file.startsWith(root + sep)) throw new Error("fuera de public/");
    const body = await readFile(file);
    response.writeHead(200, { ...headers, "Content-Type": TYPES[extname(file)] ?? "application/octet-stream" });
    response.end(body);
  } catch {
    response.writeHead(404, { "Content-Type": "text/plain; charset=utf-8" });
    response.end("404: no encontrado");
  }
}).listen(port, "127.0.0.1", () => console.log(`OpenLight web en http://localhost:${port}`));
