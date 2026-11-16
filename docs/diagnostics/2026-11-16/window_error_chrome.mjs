// The window error event probe in the pinned Chrome (CfT 148), headless, two local servers (two origins).
// node window_error_chrome.mjs <chrome executable> <out.json>   (PW_FROM or cwd must resolve playwright-core)
import { createRequire } from "module";
import http from "http";
import fs from "fs";
import path from "path";
const require = createRequire(process.env.PW_FROM || import.meta.url);
const { chromium } = require("playwright-core");
const [exe, out] = process.argv.slice(2);
const dir = path.dirname(new URL(import.meta.url).pathname);
function serve() {
  return new Promise(resolve => {
    const srv = http.createServer((q, s) => {
      const name = q.url.split("?")[0].slice(1) || "window_error.html";
      if (!/^window_error(_[a-z]+)?\.(html|js)$/.test(name)) { s.statusCode = 404; s.end(); return; }
      s.setHeader("content-type", name.endsWith(".js") ? "text/javascript" : "text/html");
      if (!fs.existsSync(path.join(dir, name))) { s.statusCode = 404; s.end(); return; }
      s.end(fs.readFileSync(path.join(dir, name)));
    }).listen(0, "127.0.0.1", () => resolve(srv));
  });
}
const [a, b] = [await serve(), await serve()];
const br = await chromium.launch({ executablePath: exe, headless: true, args: ["--disable-gpu"] });
const p = await br.newPage();
await p.goto(`http://127.0.0.1:${a.address().port}/window_error.html?other=http://127.0.0.1:${b.address().port}`);
await p.waitForFunction("window.__out", null, { timeout: 60000 });
const r = JSON.parse(await p.evaluate("window.__out"));
r.browser = br.version();
fs.writeFileSync(out, JSON.stringify(r, null, 1));
console.log(r.browser);
await br.close(); a.close(); b.close();
