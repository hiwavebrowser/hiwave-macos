// STR-4 W3: blob: and data: worker scripts in the pinned Chrome (CfT 148).
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, out] = process.argv.slice(2);
const SRC = "onmessage = function (e) { postMessage({ echo: e.data, proto: self.location.protocol, nm: self.name }); }; postMessage('up');";
const PAGE = `<!doctype html><meta charset=utf-8><body><script>
var res = {};
function go(key, url, opts) {
  var log = res[key] = [], w = new Worker(url, opts);
  w.onmessage = function (e) { log.push(e.data); };
  w.onerror = function (e) { log.push('error:' + e.constructor.name); };
  w.postMessage(key);
  return w;
}
var blob = new Blob([${JSON.stringify(SRC)}], { type: 'text/javascript' });
var burl = URL.createObjectURL(blob);
res.urlShape = [burl.indexOf('blob:' + location.origin + '/') === 0, typeof URL.revokeObjectURL];
go('blob', burl, { name: 'bn' });
var u2 = URL.createObjectURL(blob); go('revokedAfterNew', u2); URL.revokeObjectURL(u2);
go('data', 'data:text/javascript,' + encodeURIComponent(${JSON.stringify(SRC)}));
go('dataB64', 'data:text/javascript;base64,' + btoa(${JSON.stringify(SRC)}));
var u3 = URL.createObjectURL(blob); URL.revokeObjectURL(u3);
setTimeout(function () { go('revokedBefore', u3); }, 0);
go('typedBlobHtml', URL.createObjectURL(new Blob(['postMessage(1)'], { type: 'text/html' })));
setTimeout(function () { window.__out = JSON.stringify(res); }, 600);
</script>`;
const srv = http.createServer((q, r) => { r.setHeader("content-type", "text/html"); r.end(PAGE); }).listen(0, "127.0.0.1");
await new Promise(r => srv.on("listening", r));
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
await p.goto(`http://127.0.0.1:${srv.address().port}/`);
await p.waitForFunction("window.__out", { timeout: 10000 });
fs.writeFileSync(out, JSON.stringify(JSON.parse(await p.evaluate("window.__out")), null, 1));
console.log(await b.version());
await b.close(); srv.close();
