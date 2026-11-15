// STR-4 R1 nits on #1105, in the pinned Chrome (CfT 148).
// (1) A document with an opaque origin (a sandboxed iframe, a data: URL page) constructing workers from
//     http(s), data: and blob: URLs: which throw, and what.
// (2) A clone whose object has a "__proto__" key: an own property, or the prototype?
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, out] = process.argv.slice(2);
const TRY = `function __try(url) {
  var r = { threw: false }, w;
  try { w = new Worker(url); } catch (e) { r = { threw: true, name: e && e.name, code: e && e.code, message: String(e && e.message) }; }
  if (w) { try { w.terminate(); } catch (_) {} }
  return r;
}
function __cases(base) {
  var blob = URL.createObjectURL(new Blob(['postMessage(1)'], { type: 'text/javascript' }));
  return {
    origin: String(location.origin === undefined ? self.origin : self.origin),
    http: __try(base + '/w.js'),
    data: __try('data:text/javascript,postMessage(1)'),
    blob: __try(blob),
    blobShape: blob.slice(0, 10)
  };
}`;
const WORKER_PROTO = `postMessage(JSON.parse('{"__proto__":{"x":1},"y":2}'));
postMessage([JSON.parse('{"__proto__":{"x":1}}')]);`;
let base;
const srv = http.createServer((q, r) => {
  if (q.url === "/w.js") { r.setHeader("content-type", "text/javascript"); r.end("postMessage(1)"); }
  else if (q.url === "/proto.js") { r.setHeader("content-type", "text/javascript"); r.end(WORKER_PROTO); }
  else if (q.url === "/sandboxed") {
    r.setHeader("content-type", "text/html");
    r.end(`<!doctype html><script>${TRY}; parent.postMessage(JSON.stringify(__cases(${JSON.stringify(base)})), '*');</script>`);
  } else {
    r.setHeader("content-type", "text/html");
    r.end(`<!doctype html><body><script>
window.__sandboxed = new Promise(function (res) { addEventListener('message', function (e) { res(e.data); }); });
var f = document.createElement('iframe'); f.sandbox = 'allow-scripts'; f.src = '/sandboxed'; document.body.appendChild(f);
window.__proto = new Promise(function (res) {
  var w = new Worker('/proto.js'), got = [];
  w.onmessage = function (e) {
    got.push(e.data);
    if (got.length === 2) {
      var o = got[0], a = got[1][0];
      res({
        objKeys: Object.keys(o), objOwnProto: Object.prototype.hasOwnProperty.call(o, '__proto__'), objX: o.x === undefined ? 'undefined' : o.x,
        objProtoIsObjectProto: Object.getPrototypeOf(o) === Object.prototype, objOwnProtoX: o.hasOwnProperty('__proto__') ? Object.getOwnPropertyDescriptor(o, '__proto__').value.x : null,
        inArrayKeys: Object.keys(a), inArrayOwnProto: Object.prototype.hasOwnProperty.call(a, '__proto__'), inArrayX: a.x === undefined ? 'undefined' : a.x
      });
    }
  };
});
</script>`);
  }
}).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
base = `http://127.0.0.1:${srv.address().port}`;
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.goto(base + "/");
const res = {};
res.sandboxedIframe = JSON.parse(await p.evaluate("window.__sandboxed"));
res.protoKey = await p.evaluate("window.__proto");
// A top-level data: URL document (its origin is opaque too).
const p2 = await b.newPage();
await p2.goto("data:text/html," + encodeURIComponent(`<!doctype html><script>${TRY}; window.__out = JSON.stringify(__cases(${JSON.stringify(base)}));</script>`));
res.dataUrlPage = JSON.parse(await p2.evaluate("window.__out"));
const scrub = v => JSON.parse(JSON.stringify(v).split(String(srv.address().port)).join("PORT"));
fs.writeFileSync(out, JSON.stringify(scrub(res), null, 1));
console.log(await b.version());
await b.close(); srv.close();
