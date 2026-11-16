// The canvas pixels probe in the pinned Chrome (CfT 148), headless, local server only.
// node canvas_pixels_chrome.mjs <chrome executable> <out.json>   (PW_FROM or cwd must resolve playwright-core)
import { createRequire } from "module";
import http from "http";
import fs from "fs";
import path from "path";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out] = process.argv.slice(2);
const dir = path.dirname(new URL(import.meta.url).pathname);
const srv = http.createServer((q, s) => { s.setHeader("content-type", "text/html"); s.end(fs.readFileSync(path.join(dir, "canvas_pixels.html"))); }).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", null, { timeout: 15000 });
const r = { browser: b.version(), cases: JSON.parse(await p.evaluate("window.__out")) };
fs.writeFileSync(out, JSON.stringify(r, null, 1));
console.log(r.browser, Object.keys(r.cases).length);
await b.close(); srv.close();
