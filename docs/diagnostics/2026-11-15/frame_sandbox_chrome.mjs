// A page's same-origin frames with a `sandbox` attribute in the pinned Chrome (CfT 148), headless, local server only.
// node frame_sandbox_chrome.mjs <chrome executable> <out.json>   (NODE_PATH or cwd must resolve playwright-core)
import { createRequire } from "module";
import http from "http";
import fs from "fs";
import path from "path";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out] = process.argv.slice(2);
const dir = path.dirname(new URL(import.meta.url).pathname);
const srv = http.createServer((q, s) => {
  const name = q.url === "/" ? "frame_sandbox_parent.html" : q.url.slice(1).split("?")[0];
  if (!/^frame_sandbox_[a-z]+\.(html|js)$/.test(name)) { s.statusCode = 404; s.end(); return; }
  s.setHeader("content-type", name.endsWith(".js") ? "text/javascript" : "text/html");
  s.end(fs.readFileSync(path.join(dir, name)));
}).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", null, { timeout: 15000 });
// The page's own origin (a port chosen per run) is written as "page".
const origin = `http://127.0.0.1:${srv.address().port}`;
const r = JSON.parse((await p.evaluate("window.__out")).split(origin).join("page"));
r.browser = b.version();
fs.writeFileSync(out, JSON.stringify(r, null, 1));
console.log(r.browser);
await b.close(); srv.close();
