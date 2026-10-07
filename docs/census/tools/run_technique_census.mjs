#!/usr/bin/env node
// run_technique_census.mjs — TECHNIQUE CENSUS runner (docs-only tooling; no engine code).
// One Chromium instance, one load per site, sequential. Universe: websuite/realsite-top80.json.
// Usage: node docs/census/tools/run_technique_census.mjs [--sites google,ebay] [--out path.json]
// Chromium: PARITY_CHROME_PATH if set, else the Playwright-bundled Chromium from tools/parity_oracle.
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, '../../..');
const require = createRequire(join(REPO, 'tools/parity_oracle/package.json'));
const { chromium } = require('playwright');

const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const today = new Date().toISOString().slice(0, 10);
const outPath = resolve(opt('--out', join(REPO, `docs/census/technique_census_${today}.json`)));
const list = JSON.parse(readFileSync(join(REPO, 'websuite/realsite-top80.json'), 'utf8'));
const only = opt('--sites', '');
const sites = only ? list.sites.filter(s => only.split(',').includes(s.id)) : list.sites;
const classifier = readFileSync(join(__dirname, 'technique_classifier.js'), 'utf8');

const IO_HOOK = `(() => { const O = window.IntersectionObserver; if (!O) return; const c = window.__censusIO = { ctor: 0, observe: 0 };
  window.IntersectionObserver = function (cb, o) { c.ctor++; const io = new O(cb, o); const ob = io.observe.bind(io); io.observe = (t) => { c.observe++; return ob(t); }; return io; };
  window.IntersectionObserver.prototype = O.prototype; })();`;

const executablePath = process.env.PARITY_CHROME_PATH || undefined;
const browser = await chromium.launch({ headless: true, executablePath });
const meta = { date: today, started_utc: new Date().toISOString(), chromium: browser.version(), executablePath: executablePath || 'playwright-bundled', viewport: list.viewport, universe: 'websuite/realsite-top80.json', n_sites: sites.length };
const results = [];
for (const s of sites) {
  const ctx = await browser.newContext({ viewport: list.viewport, deviceScaleFactor: 1, locale: 'en-US', timezoneId: 'America/New_York' });
  await ctx.addInitScript(IO_HOOK);
  const page = await ctx.newPage();
  const rec = { id: s.id, url: s.url, status: 'NOT RUN', reason: null };
  const t0 = Date.now();
  try {
    const resp = await page.goto(s.url, { waitUntil: 'domcontentloaded', timeout: 30000 });
    rec.http_status = resp ? resp.status() : null;
    await page.waitForLoadState('networkidle', { timeout: 8000 }).catch(() => {});
    await page.waitForTimeout(1500);
    rec.data = await page.evaluate(`(${classifier})()`);
    rec.status = 'OK';
  } catch (e) {
    rec.reason = String(e && e.message || e).split('\n')[0].slice(0, 200);
  }
  rec.ms = Date.now() - t0;
  results.push(rec);
  console.error(`${rec.status.padEnd(7)} ${s.id} ${rec.ms}ms ${rec.reason || ''}`);
  await ctx.close();
}
await browser.close();
meta.finished_utc = new Date().toISOString();
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, JSON.stringify({ meta, results }, null, 1));
console.error(`wrote ${outPath}`);
