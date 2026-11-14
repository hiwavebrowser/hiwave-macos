// A dedicated worker started from a URL, and importScripts, through the network, in the pinned Chrome
// (CfT 148). The R1 HOLD on #1105 found that the engine refused both requests. A local server serves the
// page, the worker scripts and the imported scripts; each case is one page that logs what it hears.
// Usage: node worker_url_e2e_chrome.mjs <chrome.exe> <results.json>
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, resultsOut] = process.argv.slice(2);
const JS = "text/javascript";
const routes = {
  "/w.js": [JS, "importScripts('/lib.js'); postMessage('ready:' + libValue); onmessage = function (e) { postMessage('echo:' + e.data); };"],
  "/lib.js": [JS, "var libValue = 'lib';"],
  "/plain.js": ["text/plain", "postMessage('ran');"],
  "/imports_plain.js": [JS, "try { importScripts('/lib_plain.js'); postMessage('imported'); } catch (e) { postMessage('import:' + e.name); }"],
  "/lib_plain.js": ["text/plain", "var libValue = 'plain';"],
  "/imports_redirect.js": [JS, "importScripts('/redir_lib.js'); postMessage('ready:' + libValue);"],
  "/imports_missing.js": [JS, "try { importScripts('/missing.js'); postMessage('imported'); } catch (e) { postMessage('import:' + e.name); }"],
};
const redirects = { "/redir_lib.js": "/lib.js", "/redir_w.js": "/w.js" };
const server = http.createServer((req, res) => {
  const path = req.url.split("?")[0];
  if (redirects[path]) { res.writeHead(302, { location: redirects[path] }); return res.end(); }
  if (path === "/") { res.writeHead(200, { "content-type": "text/html" }); return res.end("<!doctype html><title>w</title>"); }
  const r = routes[path];
  if (!r) { res.writeHead(404, { "content-type": "text/plain" }); return res.end("no"); }
  res.writeHead(200, { "content-type": r[0] });
  res.end(r[1]);
});
await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
const origin = `http://127.0.0.1:${server.address().port}`;
const cases = {
  url_worker_with_import_scripts: "/w.js",
  worker_script_served_as_text_plain: "/plain.js",
  import_scripts_served_as_text_plain: "/imports_plain.js",
  import_scripts_through_a_same_origin_redirect: "/imports_redirect.js",
  import_scripts_of_a_missing_script: "/imports_missing.js",
  worker_script_through_a_same_origin_redirect: "/redir_w.js",
};
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
const res = {};
for (const [k, path] of Object.entries(cases)) {
  await p.goto(origin + "/");
  res[k] = await p.evaluate((path) => new Promise((done) => {
    const log = [];
    const w = new Worker(path);
    w.onmessage = (e) => { log.push("msg:" + e.data); if (String(e.data).startsWith("ready")) w.postMessage("hi"); };
    w.onerror = (e) => { log.push("error:" + e.constructor.name); e.preventDefault && e.preventDefault(); };
    setTimeout(() => done(log), 1500);
  }), path);
}
fs.writeFileSync(resultsOut, JSON.stringify(res, null, 1));
console.log(await b.version());
await b.close();
server.close();
