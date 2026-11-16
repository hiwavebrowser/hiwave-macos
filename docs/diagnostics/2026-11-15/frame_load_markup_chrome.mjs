// The iframe load probe for markup frames (frame_load_markup.html) in the pinned Chrome (CfT 148), headless, local server only.
// node frame_load_markup_chrome.mjs <chrome executable> <out.json>   (PW_FROM or cwd must resolve playwright-core)
import { createRequire } from "module";
import http from "http";
import fs from "fs";
import path from "path";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out] = process.argv.slice(2);
const dir = path.dirname(new URL(import.meta.url).pathname);
const srv = http.createServer((q, s) => {
  const name = q.url === "/" ? "frame_load_markup.html" : q.url.slice(1);
  if (!/^frame_load_[a-z]+\.(html|js)$/.test(name)) { s.statusCode = 404; s.end(); return; }
  s.setHeader("content-type", name.endsWith(".js") ? "text/javascript" : "text/html");
  s.end(fs.readFileSync(path.join(dir, name)));
}).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", null, { timeout: 15000 });
const r = JSON.parse(await p.evaluate("window.__out"));
r.browser = b.version();
fs.writeFileSync(out, JSON.stringify(r, null, 1));
console.log(r.browser);
await b.close(); srv.close();
