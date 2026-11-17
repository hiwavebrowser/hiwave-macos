// The canvas image data probe in the pinned Chrome (CfT 148), headless, local server only.
// node image_data_chrome.mjs <chrome executable> <out.json> [page, default image_data.html]   (PW_FROM or cwd must resolve playwright-core)
import { createRequire } from "module";
import http from "http";
import fs from "fs";
import path from "path";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out, page = "image_data.html"] = process.argv.slice(2);
const dir = path.dirname(new URL(import.meta.url).pathname);
const srv = http.createServer((q, s) => {
  const name = q.url === "/" ? page : q.url.slice(1);
  if (!/^image_data(_[a-z]+)?\.(html|svg|png)$/.test(name) || !fs.existsSync(path.join(dir, name))) { s.statusCode = 404; s.end(); return; }
  s.setHeader("content-type", name.endsWith(".svg") ? "image/svg+xml" : name.endsWith(".png") ? "image/png" : "text/html");
  s.end(fs.readFileSync(path.join(dir, name)));
}).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", null, { timeout: 60000 });
const r = JSON.parse(await p.evaluate("window.__out"));
r.browser = b.version();
fs.writeFileSync(out, JSON.stringify(r, null, 1));
console.log(r.browser);
await b.close(); srv.close();
