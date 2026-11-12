// STR-4 W2: timers, importScripts, uncaught errors and load failure of a
// classic dedicated worker in the pinned Chrome (CfT 148).
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, out] = process.argv.slice(2);
const FILES = {
  "/timers.js": `
    var order = [];
    setTimeout(function () { order.push('t20'); }, 20);
    setTimeout(function () { order.push('t0'); }, 0);
    var n = 0, iv = setInterval(function () { order.push('i' + (++n)); if (n === 2) clearInterval(iv); }, 5);
    var c = setTimeout(function () { order.push('cancelled'); }, 1); clearTimeout(c);
    Promise.resolve().then(function () { order.push('micro'); });
    setTimeout(function (a, b) { order.push('args:' + a + b); postMessage(order); }, 60, 'x', 'y');
  `,
  "/lib.js": `var fromLib = 'lib-ran'; function libFn() { return 42; }`,
  "/imp.js": `
    var r = [];
    importScripts('/lib.js');
    r.push(fromLib, libFn());
    importScripts('/lib.js', '/lib2.js');
    r.push(typeof fromLib2, importScripts());
    try { importScripts('/missing.js'); } catch (e) { r.push('missing:' + e.name); }
    postMessage(r);
  `,
  "/lib2.js": `var fromLib2 = 'lib2-ran';`,
  "/throws.js": `
    onmessage = function () { throw new Error('boom'); };
    postMessage('up');
  `,
  "/throwsnow.js": `postMessage('before'); throw new TypeError('top-level');`,
  "/syntax.js": `var = ;`,
  "/err.js": `
    self.onerror = function () { postMessage('self.onerror'); };
    postMessage('ok');
  `,
};
const PAGE = `<!doctype html><meta charset=utf-8><body><script>
var res = {};
function run(key, url, post, wait, cb) {
  var w = new Worker(url), log = []; res[key] = log;
  w.onmessage = function (e) { log.push(['m', e.data]); };
  w.onerror = function (e) { log.push(['e', e.message, typeof e.filename, typeof e.lineno, e instanceof ErrorEvent, e.defaultPrevented]); e.preventDefault(); };
  if (post) w.postMessage(post);
  return w;
}
run('timers', '/timers.js');
run('imp', '/imp.js');
run('throws', '/throws.js', 'go');
run('throwsnow', '/throwsnow.js');
run('syntax', '/syntax.js');
run('nofile', '/nofile.js');
run('selferr', '/err.js');
var named = new Worker('/lib.js', { name: 'nm' });
setTimeout(function () { window.__out = JSON.stringify(res); }, 600);
</script>`;
const srv = http.createServer((q, r) => {
  if (FILES[q.url]) { r.setHeader("content-type", "text/javascript"); r.end(FILES[q.url]); }
  else if (q.url === "/") { r.setHeader("content-type", "text/html"); r.end(PAGE); }
  else { r.statusCode = 404; r.end("no"); }
}).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", { timeout: 10000 });
fs.writeFileSync(out, JSON.stringify(JSON.parse(await p.evaluate("window.__out")), null, 1));
console.log(await b.version());
await b.close(); srv.close();
