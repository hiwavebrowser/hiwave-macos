// The canvas-image-in-a-frame probe in the pinned Chrome (CfT 148), headless, local server only: the
// page at 127.0.0.1, the other origin the same server as localhost.
// node frame_canvas_chrome.mjs <chrome executable> <out.json>   (PW_FROM or cwd must resolve playwright-core)
import { createRequire } from "module";
import http from "http";
import fs from "fs";
import path from "path";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out] = process.argv.slice(2);
const dir = path.dirname(new URL(import.meta.url).pathname);
const srv = http.createServer((q, s) => {
  const name = q.url.split("?")[0].slice(1);
  const to = { frame_canvas_to_same: "/frame_canvas_shape.png", frame_canvas_to_other: `http://localhost:${srv.address().port}/frame_canvas_shape.png`,
               frame_canvas_back: `http://127.0.0.1:${srv.address().port}/frame_canvas_shape.png` }[name];
  if (to) { s.statusCode = 302; s.setHeader("location", to); s.setHeader("cache-control", "no-store"); s.end(); return; }
  if (!/^frame_canvas(_[a-z]+)?\.(html|svg|png)$/.test(name) || !fs.existsSync(path.join(dir, name))) { s.statusCode = 404; s.end(); return; }
  s.setHeader("content-type", name.endsWith(".svg") ? "image/svg+xml" : name.endsWith(".png") ? "image/png" : "text/html");
  s.end(fs.readFileSync(path.join(dir, name)));
}).listen(0);
await new Promise(r => srv.on("listening", r));
const port = srv.address().port, other = encodeURIComponent(`http://localhost:${port}`);
const b = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const r = { browser: b.version() };
for (const page of ["frame_canvas.html", "frame_canvas_inner.html"]) {
  const p = await b.newPage();
  await p.goto(`http://127.0.0.1:${port}/${page}#other=${other}&redirects`);
  await p.waitForFunction("window.__out", null, { timeout: 15000 });
  Object.assign(r, JSON.parse(await p.evaluate("window.__out")));
  await p.close();
}
fs.writeFileSync(out, JSON.stringify(r, null, 1));
console.log(r.browser);
await b.close(); srv.close();
