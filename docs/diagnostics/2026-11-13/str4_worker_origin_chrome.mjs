// STR-4: which Worker script URLs throw at construction, in the pinned Chrome (CfT 148).
// Serves a page on one loopback port and scripts on a second one (another origin), and records for each
// URL whether `new Worker(url)` throws synchronously, and what it throws.
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, out] = process.argv.slice(2);
const SCRIPT = "postMessage('hi');";
const PAGE = `<!doctype html><meta charset=utf-8><body><script>
window.__try = function (url) {
  var r = { threw: false };
  var w;
  try { w = new Worker(url); } catch (e) { r = { threw: true, name: e && e.name, code: e && e.code, isDOMException: e instanceof DOMException, message: String(e && e.message) }; }
  if (w) { try { w.terminate(); } catch (_) {} }
  return r;
};
</script>`;
const other = http.createServer((q, r) => {
  r.setHeader("content-type", "text/javascript");
  if (q.url.startsWith("/cors")) r.setHeader("access-control-allow-origin", "*");
  r.end(SCRIPT);
}).listen(0, "127.0.0.1");
await new Promise(r => other.on("listening", r));
const otherPort = other.address().port;
const main = http.createServer((q, r) => {
  if (q.url === "/w.js") { r.setHeader("content-type", "text/javascript"); r.end(SCRIPT); }
  else { r.setHeader("content-type", "text/html"); r.end(PAGE); }
}).listen(0, "127.0.0.1");
await new Promise(r => main.on("listening", r));
const mainPort = main.address().port;
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${mainPort}/`);
const cases = {
  sameOriginRelative: "/w.js",
  sameOriginAbsolute: `http://127.0.0.1:${mainPort}/w.js`,
  otherPort: `http://127.0.0.1:${otherPort}/w.js`,
  otherPortWithCors: `http://127.0.0.1:${otherPort}/cors.js`,
  otherHost: `http://localhost:${mainPort}/w.js`,
  otherScheme: `https://127.0.0.1:${mainPort}/w.js`,
  dataUrl: "data:text/javascript,postMessage(1)",
  javascriptUrl: "javascript:postMessage(1)",
  malformed: "http://",
  emptyString: "",
};
const res = {};
for (const [k, v] of Object.entries(cases)) res[k] = await p.evaluate(u => window.__try(u), v);
res.blobSameOrigin = await p.evaluate(() => { var u = URL.createObjectURL(new Blob(["postMessage(1)"], { type: "text/javascript" })); return window.__try(u); });
// Messages in the message text carry the loopback ports; keep the shape, not the port.
const scrub = s => typeof s === "string" ? s.split(String(mainPort)).join("MAIN").split(String(otherPort)).join("OTHER") : s;
for (const v of Object.values(res)) if (v.message) v.message = scrub(v.message);
fs.writeFileSync(out, JSON.stringify(res, null, 1));
console.log(await b.version());
await b.close(); main.close(); other.close();
