// unhandledrejection / rejectionhandled in the pinned Chrome (CfT 148), headless, local server only
// (loopback: a file: script is cross-origin to its page, and Chrome fires no rejection event for one).
// node unhandled_rejection_chrome.mjs <chrome executable> <out.json>
// playwright-core is resolved from PW_FROM (a path inside a directory that has it, e.g.
// tools/parity_oracle/package.json after `npm ci`) or from this file's directory.
// `pageErrors` is what Chrome reported as uncaught (the console's "Uncaught (in promise)" lines),
// in order: a rejection whose event was canceled is not among them.
import { createRequire } from "module";
import http from "http";
import fs from "fs";
import path from "path";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out] = process.argv.slice(2);
const dir = path.dirname(new URL(import.meta.url).pathname);
const b = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const p = await b.newPage();
const pageErrors = [];
p.on("pageerror", e => pageErrors.push(String(e && e.message !== undefined ? e.message : e)));
const srv = http.createServer((q, s) => {
  const name = q.url === "/" ? "unhandled_rejection.html" : q.url.slice(1);
  if (!/^unhandled_rejection(_page)?\.(html|js)$/.test(name)) { s.statusCode = 404; s.end(); return; }
  s.setHeader("content-type", name.endsWith(".js") ? "text/javascript" : "text/html");
  s.end(fs.readFileSync(path.join(dir, name)));
}).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", null, { timeout: 30000 });
const r = { browser: b.version(), out: JSON.parse(await p.evaluate("window.__out")), pageErrors };
fs.writeFileSync(out, JSON.stringify(r, null, 1) + "\n");
console.log(r.browser);
await b.close(); srv.close();
