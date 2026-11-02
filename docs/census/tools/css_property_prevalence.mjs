#!/usr/bin/env node
// css_property_prevalence.mjs - how many top-100 sites author each CSS property (dev-tools T13c).
// One load per site, four at a time. The output is counts per property only: no site names, no URLs,
// no page text. ebay is never loaded live, whatever the list says.
// Usage: node docs/census/tools/css_property_prevalence.mjs [--list websuite/realsite-top100.json] [--out path.json]
import { readFileSync, writeFileSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';
import { authoredProperties, isBlockedTitle, neverLoad } from './css_property_names.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, '../../..');
const require = createRequire(join(REPO, 'tools/parity_oracle/package.json'));
const { chromium } = require('playwright');

const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const listRel = opt('--list', 'websuite/realsite-top100.json');
const outPath = resolve(opt('--out', join(REPO, 'docs/census/css_property_prevalence_top100.json')));
const list = JSON.parse(readFileSync(resolve(REPO, listRel), 'utf8'));
const sites = list.sites.filter((s) => !neverLoad(s.url));
const WORKERS = 4;

// Text a stylesheet's author wrote: <style> text, rules a script inserted into an empty <style>, and
// style attributes, each attribute opened with `{` so its first declaration reads like one.
const PAGE_CSS = `(() => {
  const out = [];
  for (const el of document.querySelectorAll('style')) {
    if (el.textContent.trim()) { out.push(el.textContent); continue; }
    try { for (const r of el.sheet.cssRules) out.push(r.cssText); } catch {}
  }
  for (const el of document.querySelectorAll('[style]')) out.push('{' + el.getAttribute('style'));
  return out;
})()`;

const browser = await chromium.launch({ headless: true });

async function visit(site) {
  const ctx = await browser.newContext({ viewport: list.viewport, deviceScaleFactor: 1, locale: 'en-US', timezoneId: 'America/New_York' });
  const page = await ctx.newPage();
  const bodies = [];
  page.on('response', (resp) => {
    const req = resp.request();
    if (req.resourceType() !== 'stylesheet' || req.frame() !== page.mainFrame()) return;
    bodies.push(resp.text().catch(() => ''));
  });
  try {
    const resp = await page.goto(site.url, { waitUntil: 'domcontentloaded', timeout: 30000 });
    await page.waitForLoadState('networkidle', { timeout: 8000 }).catch(() => {});
    await page.waitForTimeout(1500);
    const status = resp ? resp.status() : 0;
    if ([401, 403, 429, 503].includes(status) || status >= 500 || isBlockedTitle(await page.title())) return null;
    const css = await page.evaluate(PAGE_CSS);
    const names = new Set();
    for (const text of [...css, ...(await Promise.all(bodies))]) {
      for (const name of authoredProperties(text)) names.add(name);
    }
    return names;
  } catch {
    return null;
  } finally {
    await ctx.close().catch(() => {});
  }
}

const counts = {};
let loaded = 0;
let done = 0;
let next = 0;
await Promise.all(Array.from({ length: WORKERS }, async () => {
  while (next < sites.length) {
    const site = sites[next++];
    const names = await visit(site);
    if (names) {
      loaded += 1;
      for (const name of names) counts[name] = (counts[name] || 0) + 1;
    }
    done += 1;
    console.error(`${done}/${sites.length} loaded ${loaded}`);
  }
}));

const version = browser.version();
await browser.close();
const sorted = Object.fromEntries(Object.entries(counts).sort(([a], [b]) => (a < b ? -1 : 1)));
writeFileSync(outPath, JSON.stringify({
  meta: {
    date: new Date().toISOString().slice(0, 10),
    chromium: version,
    universe: listRel,
    asked: sites.length,
    loaded,
    measure: 'loaded sites whose main frame authors the property in a stylesheet, <style> or style attribute',
  },
  sites_authoring: sorted,
}, null, 1) + '\n');
console.error(`wrote ${outPath}`);
