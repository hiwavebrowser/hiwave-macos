// STR-4 W1: what a classic dedicated worker does in the pinned Chrome (CfT 148).
// Serves a page and worker.js on loopback; prints the observed log as JSON.
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, out] = process.argv.slice(2);
const WORKER = `
var log = [];
self.onmessage = function (e) {
  if (e.data === 'close') { postMessage('closing'); self.close(); return; }
  if (e.data === 'info') {
    postMessage({ t: typeof self, doc: typeof document, win: typeof window, name: self.name,
      post: typeof postMessage, ael: typeof addEventListener, imp: typeof importScripts,
      st: typeof setTimeout, loc: typeof location, nav: typeof navigator });
    return;
  }
  postMessage({ echo: e.data, isEvent: e instanceof MessageEvent, tgt: e.target === self });
};
addEventListener('message', function (e) { if (e.data === 'both') postMessage('listener'); });
postMessage('ready');
`;
const PAGE = `<!doctype html><meta charset=utf-8><body><script>
var res = [], w = new Worker('/worker.js');
w.onmessage = function (e) { res.push(e.data); };
w.addEventListener('message', function (e) { if (e.data === 'ready') res.push('ael:ready'); });
w.postMessage({ a: 1, b: [1, 2, 'x'], n: null });
w.postMessage('info');
w.postMessage('both');
w.postMessage('close');
setTimeout(function () { w.postMessage('after-close'); }, 100);
setTimeout(function () {
  var w2 = new Worker('/worker.js'); var got = [];
  w2.onmessage = function (e) { got.push(e.data); };
  w2.terminate(); w2.postMessage('x');
  setTimeout(function () {
    window.__out = JSON.stringify({ log: res, afterTerminate: got, ctor: typeof Worker, proto: w instanceof EventTarget }); }, 200);
}, 300);
</script>`;
const srv = http.createServer((q, r) => {
  if (q.url === "/worker.js") { r.setHeader("content-type", "text/javascript"); r.end(WORKER); }
  else { r.setHeader("content-type", "text/html"); r.end(PAGE); }
}).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", { timeout: 10000 });
const o = await p.evaluate("window.__out");
fs.writeFileSync(out, JSON.stringify(JSON.parse(o), null, 1));
console.log(await b.version());
await b.close(); srv.close();
