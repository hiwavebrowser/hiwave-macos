// node --test docs/census/tools/prevalence_visit.test.mjs
// The prevalence crawl's drop rule, against a local server only: every host resolves to 127.0.0.1,
// so nothing leaves the machine. A site whose load asked for a refused host is dropped on every
// path (a page redirect, a blocked status, a request made while the crawl reads the page), and the
// drop is what the crawl's "sites dropped" line counts.
import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { createRequire } from 'node:module';
import { launchRefusing, neverLoad } from './never_load.mjs';
import { visitSite } from './prevalence_visit.mjs';

const require = createRequire(new URL('../../../tools/parity_oracle/package.json', import.meta.url));
const { chromium } = require('playwright');

const asked = [];
let port = 0;
const at = (host, path) => `http://${host}:${port}${path}`;
const html = (res, status, body) => {
  res.writeHead(status, { 'content-type': 'text/html' });
  res.end(body);
};

const server = http.createServer((req, res) => {
  asked.push((req.headers.host || '').split(':')[0]);
  switch (req.url) {
    case '/clean':
      return html(res, 200, '<style>p{color:red}</style><p style="margin-top:1px">x</p>');
    case '/go':
      res.writeHead(301, { location: at('www.ebay.com', '/landing') });
      return res.end();
    case '/forbidden':
      return html(res, 403, `<img src="${at('cdn.shopify.com', '/a.png')}"><p>x</p>`);
    case '/sheet':
      return html(res, 200, '<link rel=stylesheet href=/redir.css><p>x</p>');
    case '/redir.css':
      res.writeHead(302, { location: at('cdn.shopify.com', '/after.css') });
      return res.end();
    case '/during-read':
      // The crawl's own read calls document.querySelectorAll; this page asks for a refused host
      // 100 ms after that, while the crawl is still on it.
      return html(res, 200, `<style>p{color:red}</style><p>x</p><script>
        const q = Document.prototype.querySelectorAll;
        let once = false;
        document.querySelectorAll = function (s) {
          if (!once) { once = true; setTimeout(() => { new Image().src = ${JSON.stringify(at('www.ebay.com', '/late.png'))}; }, 100); }
          return q.call(this, s);
        };
      </script>`);
    default:
      res.writeHead(404);
      return res.end();
  }
});

let browser;
test.before(async () => {
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  port = server.address().port;
  browser = await launchRefusing(chromium, { headless: true }, ['MAP * 127.0.0.1']);
});
test.after(async () => {
  await browser.close();
  server.close();
  assert.deepEqual(asked.filter((host) => neverLoad(`http://${host}/`)), [], 'no refused host reached the server');
});

const visit = (path) => visitSite(browser, at('site.test', path), { viewport: { width: 800, height: 600 }, settleMs: 200 });

test('a clean page is loaded and read', async () => {
  const r = await visit('/clean');
  assert.equal(r.outcome, 'loaded');
  assert.deepEqual([...r.names].sort(), ['color', 'margin-top']);
});

test('a page that redirects into a refused host is dropped, not failed', async () => {
  const r = await visit('/go');
  assert.equal(r.outcome, 'dropped');
  assert.equal(r.names, null);
});

test('a blocked-status page that asked for a refused host is dropped, not failed', async () => {
  const r = await visit('/forbidden');
  assert.equal(r.outcome, 'dropped');
});

test('a stylesheet that redirects into a refused host drops the site', async () => {
  const r = await visit('/sheet');
  assert.equal(r.outcome, 'dropped');
});

test('a refused request made while the crawl reads the page drops the site', async () => {
  const r = await visit('/during-read');
  assert.equal(r.outcome, 'dropped');
  assert.equal(r.names, null);
});
