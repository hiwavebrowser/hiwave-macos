// node --test docs/census/tools/never_load.test.mjs
// Runs Chromium against a local server only: every host resolves to 127.0.0.1, so nothing leaves the
// machine, and the server records which hosts were asked for.
import test from 'node:test';
import assert from 'node:assert/strict';
import http from 'node:http';
import { createRequire } from 'node:module';
import { hostResolverRules, launchRefusing, neverLoad, watchRefused } from './never_load.mjs';

const require = createRequire(new URL('../../../tools/parity_oracle/package.json', import.meta.url));
const { chromium } = require('playwright');

const REFUSED = ['www.ebay.com', 'ebay.co.uk', 'i.ebayimg.com', 'ir.ebaystatic.com', 'vi.ebaydesc.com', 'www.ebayinc.com',
  'www.shopify.com', 'cdn.shopify.com', 'a.myshopify.com', 'x.shopifycdn.com', 'cdn.shopifycloud.com', 'm.shopifysvc.com',
  'shop.app', 'pay.shop.app'];

test('every refused host is refused by name, and near misses are not', () => {
  for (const host of REFUSED) assert.equal(neverLoad(`https://${host}/a`), true, host);
  for (const host of ['www.notshopify.com', 'notebay.com', 'workshop.app.example', 'myshop.app']) {
    assert.equal(neverLoad(`https://${host}/`), false, host);
  }
});

test('the resolver rules name every refused domain and its subdomains', () => {
  const rules = hostResolverRules();
  for (const domain of ['ebay', 'ebayimg', 'ebaystatic', 'ebaydesc', 'ebayinc', 'shopify', 'myshopify', 'shopifycdn', 'shopifycloud', 'shopifysvc']) {
    assert.ok(rules.includes(`MAP ${domain}.* ~NOTFOUND`) && rules.includes(`MAP *.${domain}.* ~NOTFOUND`), domain);
  }
  assert.ok(rules.includes('MAP shop.app ~NOTFOUND') && rules.includes('MAP *.shop.app ~NOTFOUND'));
});

test('a redirect into a refused host fails, for a page and for a stylesheet', async (t) => {
  const asked = [];
  const server = http.createServer((req, res) => {
    const host = (req.headers.host || '').split(':')[0];
    asked.push(host);
    const at = `http://%s:${server.address().port}`;
    if (req.url === '/go') {
      res.writeHead(301, { location: `${at.replace('%s', 'www.ebay.com')}/landing` });
    } else if (req.url === '/redir.css') {
      res.writeHead(302, { location: `${at.replace('%s', 'cdn.shopify.com')}/after.css` });
    } else if (req.url === '/page') {
      res.writeHead(200, { 'content-type': 'text/html' });
      return res.end('<link rel=stylesheet href=/redir.css><p>x</p>');
    } else {
      res.writeHead(200, { 'content-type': 'text/css' });
      return res.end('p{color:red}');
    }
    res.end();
  });
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  t.after(() => server.close());
  const port = server.address().port;
  const browser = await launchRefusing(chromium, { headless: true }, ['MAP * 127.0.0.1']);
  t.after(() => browser.close());

  const page1 = await (await browser.newContext()).newPage();
  const watch1 = await watchRefused(page1.context());
  await assert.rejects(page1.goto(`http://site.test:${port}/go`), /ERR_NAME_NOT_RESOLVED|ERR_FAILED|ERR_ABORTED/);
  assert.equal(watch1.touched, true, 'the page touched a refused host, so its site is dropped');

  const page2 = await (await browser.newContext()).newPage();
  const watch2 = await watchRefused(page2.context());
  const sheet = page2.waitForEvent('requestfailed', (r) => r.url().includes('after.css') || r.url().includes('redir.css'));
  await page2.goto(`http://site.test:${port}/page`);
  await sheet;
  assert.equal(watch2.touched, true, 'the stylesheet touched a refused host, so its site is dropped');

  assert.ok(asked.includes('site.test'), 'the local server was reached');
  assert.deepEqual(asked.filter((host) => neverLoad(`http://${host}/`)), [], 'no refused host reached the server');
});

test('both crawlers launch through the refusal and watch every context', async () => {
  const { readFileSync } = await import('node:fs');
  for (const file of ['css_property_prevalence.mjs', 'run_technique_census.mjs']) {
    const src = readFileSync(new URL(`./${file}`, import.meta.url), 'utf8');
    assert.match(src, /from '\.\/never_load\.mjs'/, file);
    assert.doesNotMatch(src, /chromium\.launch\(/, file);
    assert.match(src, /launchRefusing\(/, file);
    assert.match(src, /watchRefused\(/, file);
    assert.doesNotMatch(src, /ebay|shopify/i, `${file} names no refused site itself`);
  }
});
