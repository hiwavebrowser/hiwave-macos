#!/usr/bin/env node
/**
 * img_empty_src_log.mjs - Chromium's answers for CH2.E42: what an <img>
 * whose src is empty, "#", whitespace or a bare fragment requests, and
 * which event it fires.
 *
 *   PARITY_CHROME_PATH=<CfT binary> node img_empty_src_log.mjs
 *
 * Each case is its own page at /case/<id> on a local server that logs every
 * request path. Prints, per case, the requests made after the page itself
 * (favicon excluded) and the image's event (load / error / none),
 * `currentSrc` and `naturalWidth`.
 */
import { chromium } from 'playwright';
import http from 'http';

const PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==',
  'base64',
);

// id -> the <img> markup (or a script that makes one) under test.
const CASES = {
  empty: '<img id="t" src="">',
  hash: '<img id="t" src="#">',
  frag: '<img id="t" src="#top">',
  space: '<img id="t" src="   ">',
  nosrc: '<img id="t">',
  srcset_empty_src_empty: '<img id="t" srcset="" src="">',
  srcset_space: '<img id="t" srcset=" ">',
  query: '<img id="t" src="?x=1">',
  self: '<img id="t" src="/case/self">',
  ok: '<img id="t" src="/ok.png">',
  script_empty: '<script>{const i=new Image();i.id="t";i.src="";document.body.appendChild(i);}</script>',
  script_hash: '<script>{const i=new Image();i.id="t";i.src="#";document.body.appendChild(i);}</script>',
};

// Listeners sit on document: a load event never propagates to Window.
const page_html = (body) => `<!doctype html><html><head><title>e42</title><script>
window.__ev = 'none';
document.addEventListener('load', (e) => { if (e.target.id === 't') window.__ev = 'load'; }, true);
document.addEventListener('error', (e) => { if (e.target.id === 't') window.__ev = 'error'; }, true);
</script></head><body>${body}</body></html>`;

let log = [];
const server = http.createServer((req, res) => {
  log.push(req.url);
  if (req.url.startsWith('/ok.png')) {
    res.writeHead(200, { 'content-type': 'image/png' });
    res.end(PNG);
    return;
  }
  const id = req.url.split('?')[0].split('#')[0].replace('/case/', '');
  res.writeHead(200, { 'content-type': 'text/html', 'cache-control': 'no-store' });
  res.end(page_html(CASES[id] ?? ''));
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const origin = `http://127.0.0.1:${server.address().port}`;

const exe = process.env.PARITY_CHROME_PATH;
const browser = await chromium.launch(exe ? { executablePath: exe } : {});
const out = { cases: {} };
for (const id of Object.keys(CASES)) {
  const page = await browser.newPage();
  log = [];
  await page.goto(`${origin}/case/${id}`, { waitUntil: 'load' });
  await page.waitForTimeout(300);
  const r = await page.evaluate(() => {
    const i = document.getElementById('t');
    return {
      version: navigator.userAgent.match(/Chrome\/[\d.]+/)[0],
      event: window.__ev,
      currentSrc: i.currentSrc.replace(location.origin, ''),
      naturalWidth: i.naturalWidth,
    };
  });
  out.version = r.version;
  delete r.version;
  r.requests = log.slice(1).filter((u) => u !== '/favicon.ico');
  out.cases[id] = r;
  await page.close();
}
console.log(JSON.stringify(out, null, 1));
await browser.close();
server.close();
