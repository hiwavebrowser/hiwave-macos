// The #1105 follow-up's Chrome measurements (CfT 148): CSP worker-src and its fallbacks, data:/blob: workers
// under CSP, a data: import under script-src, cross-origin importScripts, nosniff, location after a redirect,
// importScripts(blob:) and a partial importScripts failure. Two local servers (A: the page; B: another origin).
// Usage: node worker_csp_probe.mjs <chrome.exe> <results.json>
import puppeteer from "puppeteer-core";
import http from "http";
import fs from "fs";
const [exe, out] = process.argv.slice(2);
const JS = "text/javascript";
let originB = "";
const scriptsA = {
  "/w.js": "postMessage('ran:' + self.location.pathname);",
  "/plain_nosniff.js": "postMessage('ran');",
  "/imports_data.js": "try { importScripts('data:text/javascript,postMessage(%22data-ran%22)'); postMessage('imported'); } catch (e) { postMessage('import:' + e.name); }",
  "/imports_cross.js": "try { importScripts(ORIGIN_B + '/lib.js'); postMessage('imported:' + self.libValue); } catch (e) { postMessage('import:' + e.name); }",
  "/imports_cross_cors.js": "try { importScripts(ORIGIN_B + '/lib_cors.js'); postMessage('imported:' + self.libValue); } catch (e) { postMessage('import:' + e.name); }",
  "/imports_partial.js": "self.order = []; try { importScripts('/a.js', '/missing.js'); postMessage('imported'); } catch (e) { postMessage('import:' + e.name + ':' + self.order.join()); }",
  "/a.js": "self.order.push('a');",
  "/imports_blob.js": "onmessage = function (e) { try { importScripts(e.data); postMessage('imported:' + self.libValue); } catch (err) { postMessage('import:' + err.name); } };",
};
const serverA = http.createServer((req, res) => {
  const u = new URL(req.url, "http://x");
  const path = u.pathname;
  if (path === "/") {
    const csp = u.searchParams.get("csp");
    const headers = { "content-type": "text/html" };
    if (csp) headers["content-security-policy"] = csp;
    res.writeHead(200, headers);
    return res.end("<!doctype html><title>w</title>");
  }
  if (path === "/redir_w.js") { res.writeHead(302, { location: "/w.js" }); return res.end(); }
  const src = scriptsA[path];
  if (!src) { res.writeHead(404, { "content-type": "text/plain" }); return res.end("no"); }
  const headers = { "content-type": path === "/plain_nosniff.js" ? "text/plain" : JS };
  if (path === "/plain_nosniff.js") headers["x-content-type-options"] = "nosniff";
  // A worker script's own CSP: the data: import case.
  if (path === "/imports_data.js") headers["content-security-policy"] = "script-src 'self'";
  res.writeHead(200, headers);
  res.end(src.replace("ORIGIN_B", JSON.stringify(originB)));
});
const serverB = http.createServer((req, res) => {
  const path = new URL(req.url, "http://x").pathname;
  const headers = { "content-type": JS };
  if (path === "/lib_cors.js") headers["access-control-allow-origin"] = "*";
  res.writeHead(200, headers);
  res.end("self.libValue = 'b';");
});
await new Promise((ok) => serverA.listen(0, "127.0.0.1", ok));
await new Promise((ok) => serverB.listen(0, "127.0.0.1", ok));
const originA = `http://127.0.0.1:${serverA.address().port}`;
originB = `http://localhost:${serverB.address().port}`;
// [page CSP or "", worker kind, worker script path or source]
const cases = {
  url_worker_no_csp: ["", "url", "/w.js"],
  url_worker_script_src_nonce_worker_src_self: ["script-src 'nonce-abc'; worker-src 'self'", "url", "/w.js"],
  url_worker_worker_src_none_script_src_star: ["worker-src 'none'; script-src *", "url", "/w.js"],
  url_worker_script_src_self_only: ["script-src 'self'", "url", "/w.js"],
  url_worker_script_src_none_only: ["script-src 'none'", "url", "/w.js"],
  url_worker_child_src_none_script_src_self: ["child-src 'none'; script-src 'self'", "url", "/w.js"],
  url_worker_child_src_self_script_src_none: ["child-src 'self'; script-src 'none'", "url", "/w.js"],
  url_worker_default_src_none: ["default-src 'none'", "url", "/w.js"],
  blob_worker_no_csp: ["", "blob", "postMessage('ran')"],
  blob_worker_worker_src_self: ["worker-src 'self'", "blob", "postMessage('ran')"],
  blob_worker_worker_src_blob: ["worker-src blob:", "blob", "postMessage('ran')"],
  data_worker_no_csp: ["", "data", "postMessage('ran')"],
  data_worker_worker_src_self: ["worker-src 'self'", "data", "postMessage('ran')"],
  data_worker_worker_src_data: ["worker-src data:", "data", "postMessage('ran')"],
  import_data_url_under_worker_script_src_self: ["", "url", "/imports_data.js"],
  import_cross_origin_without_cors: ["", "url", "/imports_cross.js"],
  import_cross_origin_with_cors: ["", "url", "/imports_cross_cors.js"],
  worker_script_text_plain_nosniff: ["", "url", "/plain_nosniff.js"],
  worker_location_after_redirect: ["", "url", "/redir_w.js"],
  import_scripts_partial_failure: ["", "url", "/imports_partial.js"],
  import_scripts_of_a_blob_url: ["", "blob_import", "/imports_blob.js"],
};
const b = await puppeteer.launch({ executablePath: exe, headless: true, args: ["--no-sandbox", "--disable-gpu"] });
const p = await b.newPage();
const res = {};
for (const [k, [csp, kind, what]] of Object.entries(cases)) {
  await p.goto(originA + "/" + (csp ? "?csp=" + encodeURIComponent(csp) : ""));
  res[k] = await p.evaluate((kind, what) => new Promise((done) => {
    const log = [];
    let w;
    try {
      if (kind === "url") w = new Worker(what);
      else if (kind === "blob") w = new Worker(URL.createObjectURL(new Blob([what], { type: "text/javascript" })));
      else if (kind === "data") w = new Worker("data:text/javascript," + encodeURIComponent(what));
      else if (kind === "blob_import") {
        w = new Worker(what);
        w.postMessage(URL.createObjectURL(new Blob(["self.libValue = 'blob';"], { type: "text/javascript" })));
      }
    } catch (e) { log.push("throw:" + e.name); done(log); return; }
    w.onmessage = (e) => log.push("msg:" + e.data);
    w.onerror = (e) => { log.push("error"); e.preventDefault(); };
    setTimeout(() => done(log), 1200);
  }), kind, what);
}
fs.writeFileSync(out, JSON.stringify(res, null, 1));
console.log(await b.version());
await b.close();
serverA.close();
serverB.close();
