// The gallery's web smoke test: serves a Trunk `dist/` and loads it in
// headless Chrome (WebGL through SwiftShader), then passes when the app
// logs that it started and fails on a panic, a WebAssembly trap or an
// uncaught error.
//
//   node tools/web-smoke.mjs <dist> [--base /egui-pop/] [--timeout 60]
//        [--chrome google-chrome] [--screenshot smoke.png]
//
// Needs Node 22 (its global WebSocket) and Chrome; nothing to install.

import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join, normalize } from "node:path";

const STARTED = "egui-pop gallery started";
const FATAL = [/panicked at/, /RuntimeError/, /unreachable executed/, /could not run in this browser/];
// After the start line, frames keep running this long without a panic.
const SETTLE_MS = 3000;

function args() {
  const a = process.argv.slice(2);
  const o = { dist: null, base: "/", timeout: 60, chrome: process.env.CHROME || "google-chrome", screenshot: null };
  for (let i = 0; i < a.length; i++) {
    const k = a[i];
    if (k === "--base") o.base = a[++i];
    else if (k === "--timeout") o.timeout = Number(a[++i]);
    else if (k === "--chrome") o.chrome = a[++i];
    else if (k === "--screenshot") o.screenshot = a[++i];
    else if (!o.dist) o.dist = k;
    else throw new Error(`unexpected argument ${k}`);
  }
  if (!o.dist) throw new Error("usage: web-smoke.mjs <dist> [--base /path/] [--timeout s] [--chrome bin] [--screenshot file]");
  if (!o.base.startsWith("/")) o.base = "/" + o.base;
  if (!o.base.endsWith("/")) o.base += "/";
  return o;
}

const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".wasm": "application/wasm",
  ".css": "text/css",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".json": "application/json",
};

function serve(dist, base) {
  const server = createServer((req, res) => {
    let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    if (!path.startsWith(base)) {
      res.writeHead(404).end();
      return;
    }
    path = normalize(path.slice(base.length)).replace(/^(\.\.[/\\])+/, "");
    let file = join(dist, path);
    try {
      if (statSync(file).isDirectory()) file = join(file, "index.html");
      const body = readFileSync(file);
      res.writeHead(200, { "content-type": TYPES[extname(file)] || "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

function launch(chrome, profile) {
  const proc = spawn(
    chrome,
    [
      "--headless=new",
      "--no-sandbox",
      "--no-first-run",
      "--no-default-browser-check",
      `--user-data-dir=${profile}`,
      "--remote-debugging-port=0",
      // WebGL without a GPU: SwiftShader, through ANGLE.
      "--use-angle=swiftshader",
      "--enable-unsafe-swiftshader",
      "--ignore-gpu-blocklist",
      "--window-size=1280,800",
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  return new Promise((resolve, reject) => {
    let err = "";
    proc.on("error", reject);
    proc.on("exit", (code) => reject(new Error(`chrome exited (${code}) before DevTools:\n${err}`)));
    proc.stderr.on("data", (d) => {
      err += d;
      const m = /DevTools listening on (ws:\/\/\S+)/.exec(err);
      if (m) resolve({ proc, port: new URL(m[1]).port });
    });
  });
}

function cdp(url) {
  const ws = new WebSocket(url);
  let next = 1;
  const waiting = new Map();
  const listeners = [];
  ws.addEventListener("message", (e) => {
    const msg = JSON.parse(e.data);
    if (msg.id && waiting.has(msg.id)) {
      const { resolve, reject } = waiting.get(msg.id);
      waiting.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    } else if (msg.method) {
      for (const l of listeners) l(msg.method, msg.params);
    }
  });
  return new Promise((resolve, reject) => {
    ws.addEventListener("error", () => reject(new Error(`cannot connect to ${url}`)));
    ws.addEventListener("open", () =>
      resolve({
        send(method, params = {}) {
          const id = next++;
          ws.send(JSON.stringify({ id, method, params }));
          return new Promise((res, rej) => waiting.set(id, { resolve: res, reject: rej }));
        },
        on(f) {
          listeners.push(f);
        },
        close() {
          ws.close();
        },
      }),
    );
  });
}

function text(arg) {
  if (arg.value !== undefined) return typeof arg.value === "string" ? arg.value : JSON.stringify(arg.value);
  return arg.description ?? arg.unserializableValue ?? `<${arg.type}>`;
}

async function main() {
  const o = args();
  const server = await serve(o.dist, o.base);
  const url = `http://127.0.0.1:${server.address().port}${o.base}`;
  const profile = mkdtempSync(join(tmpdir(), "web-smoke-"));
  const { proc, port } = await launch(o.chrome, profile);
  let verdict = null;
  let started = false;
  const lines = [];
  try {
    const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
    const page = targets.find((t) => t.type === "page");
    const c = await cdp(page.webSocketDebuggerUrl);
    const fail = (why) => {
      if (!verdict) verdict = { ok: false, why };
    };
    const seen = (line) => {
      lines.push(line);
      console.log(line);
      if (FATAL.some((re) => re.test(line))) fail(`fatal message: ${line}`);
      if (line.includes(STARTED)) started = true;
    };
    c.on((method, p) => {
      if (method === "Runtime.consoleAPICalled") {
        seen(`console.${p.type}: ${p.args.map(text).join(" ")}`);
      } else if (method === "Runtime.exceptionThrown") {
        const d = p.exceptionDetails;
        const line = `uncaught: ${d.exception?.description ?? d.text}`;
        seen(line);
        fail(line);
      } else if (method === "Log.entryAdded") {
        seen(`log.${p.entry.level}: ${p.entry.text}${p.entry.url ? ` (${p.entry.url})` : ""}`);
      }
    });
    await c.send("Runtime.enable");
    await c.send("Log.enable");
    await c.send("Page.enable");
    console.log(`loading ${url}`);
    await c.send("Page.navigate", { url });
    const deadline = Date.now() + o.timeout * 1000;
    let startedAt = null;
    while (!verdict && Date.now() < deadline) {
      if (started && startedAt === null) startedAt = Date.now();
      if (startedAt !== null && Date.now() - startedAt >= SETTLE_MS) {
        verdict = { ok: true, why: `"${STARTED}" logged, no panic in the ${SETTLE_MS} ms after` };
      }
      await new Promise((r) => setTimeout(r, 100));
    }
    if (!verdict) verdict = { ok: false, why: `no "${STARTED}" within ${o.timeout} s` };
    if (o.screenshot) {
      try {
        const shot = await c.send("Page.captureScreenshot", { format: "png" });
        writeFileSync(o.screenshot, Buffer.from(shot.data, "base64"));
        console.log(`screenshot: ${o.screenshot}`);
      } catch (e) {
        console.log(`no screenshot: ${e.message}`);
      }
    }
    c.close();
  } finally {
    proc.kill("SIGKILL");
    server.close();
    try {
      rmSync(profile, { recursive: true, force: true });
    } catch {}
  }
  console.log(`${verdict.ok ? "PASS" : "FAIL"}: ${verdict.why}`);
  process.exit(verdict.ok ? 0 : 1);
}

main().catch((e) => {
  console.error(`FAIL: ${e.stack ?? e}`);
  process.exit(1);
});
