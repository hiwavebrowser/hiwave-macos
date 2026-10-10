#!/usr/bin/env node
// run_technique_census.mjs — TECHNIQUE CENSUS runner (docs-only tooling; no engine code).
// One Chromium instance, one load per site, sequential. Universe: websuite list JSON (default realsite-top80.json).
// Usage: node docs/census/tools/run_technique_census.mjs [--list websuite/realsite-top100.json] [--sites google,ebay] [--out path.json]
// Chromium: PARITY_CHROME_PATH if set, else Playwright-bundled Chromium.
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';
import { createRequire } from 'module';
import { homedir } from 'os';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, '../../..');
const require = createRequire(join(REPO, 'tools/parity_oracle/package.json'));
const { chromium } = require('playwright');

const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const today = new Date().toISOString().slice(0, 10);
const outPath = resolve(opt('--out', join(REPO, `docs/census/technique_census_${today}.json`)));
const listRel = opt('--list', 'websuite/realsite-top80.json');
const listPath = resolve(REPO, listRel);
const list = JSON.parse(readFileSync(listPath, 'utf8'));
const only = opt('--sites', '');
const sites = only ? list.sites.filter(s => only.split(',').includes(s.id)) : list.sites;
const universeLabel = listRel.startsWith('/') ? listPath : listRel;
const classifier = readFileSync(join(__dirname, 'technique_classifier.js'), 'utf8');

const IO_HOOK = `(() => { const O = window.IntersectionObserver; if (!O) return; const c = window.__censusIO = { ctor: 0, observe: 0 };
  window.IntersectionObserver = function (cb, o) { c.ctor++; const io = new O(cb, o); const ob = io.observe.bind(io); io.observe = (t) => { c.observe++; return ob(t); }; return io; };
  window.IntersectionObserver.prototype = O.prototype; })();`;

function resolveChrome() {
  if (process.env.PARITY_CHROME_PATH && existsSync(process.env.PARITY_CHROME_PATH)) {
    return { executablePath: process.env.PARITY_CHROME_PATH, source: 'PARITY_CHROME_PATH' };
  }
  // Prefer Playwright's bundled Chromium from cache (headless-friendly)
  const cache = join(homedir(), 'Library/Caches/ms-playwright');
  if (existsSync(cache)) {
    const dirs = readdirSync(cache).filter(d => d.startsWith('chromium-') && !d.includes('headless_shell')).sort().reverse();
    for (const d of dirs) {
      const mac = join(cache, d, 'chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing');
      const mac2 = join(cache, d, 'chrome-mac/Chromium.app/Contents/MacOS/Chromium');
      if (existsSync(mac)) return { executablePath: mac, source: `playwright-bundled:${d}` };
      if (existsSync(mac2)) return { executablePath: mac2, source: `playwright-bundled:${d}` };
    }
    const shells = readdirSync(cache).filter(d => d.startsWith('chromium_headless_shell-')).sort().reverse();
    for (const d of shells) {
      const sh = join(cache, d, 'chrome-headless-shell-mac-arm64/chrome-headless-shell');
      if (existsSync(sh)) return { executablePath: sh, source: `playwright-headless-shell:${d}` };
    }
  }
  return { executablePath: undefined, source: 'playwright-default' };
}

function classifyBotWallFromHeaders(headers, status) {
  const h = {};
  for (const [k, v] of Object.entries(headers || {})) h[k.toLowerCase()] = String(v);
  const vendors = [];
  const signals = [];
  const server = (h['server'] || '').toLowerCase();
  const via = (h['via'] || '').toLowerCase();
  if (h['cf-ray'] || server.includes('cloudflare') || h['cf-mitigated']) { vendors.push('cloudflare'); signals.push('header'); }
  if (server.includes('akamai') || via.includes('akamai') || h['x-akamai-transformed'] || h['akamai-grn']) { vendors.push('akamai'); signals.push('header'); }
  if (h['x-datadome'] || h['x-dd-b'] || (h['set-cookie'] || '').toLowerCase().includes('datadome')) { vendors.push('datadome'); signals.push('header'); }
  if (h['x-px'] || (h['set-cookie'] || '').toLowerCase().includes('_px') || server.includes('perimeterx')) { vendors.push('perimeterx'); signals.push('header'); }
  return { vendors: [...new Set(vendors)], signals, status };
}

function isJsResource(url, headers) {
  const u = (url || '').split('?')[0].toLowerCase();
  const ct = ((headers && (headers['content-type'] || headers['Content-Type'])) || '').toLowerCase();
  if (/\.m?js$/.test(u) || /\.mjs$/.test(u)) return true;
  if (ct.includes('javascript') || ct.includes('ecmascript')) return true;
  return false;
}

const chrome = resolveChrome();
const launchOpts = { headless: true };
if (chrome.executablePath) launchOpts.executablePath = chrome.executablePath;
const browser = await chromium.launch(launchOpts);
const meta = {
  date: today,
  started_utc: new Date().toISOString(),
  chromium: browser.version(),
  executablePath: chrome.executablePath || 'playwright-bundled-default',
  chrome_source: chrome.source,
  viewport: list.viewport,
  universe: universeLabel,
  n_sites: sites.length,
};
const results = [];
const MAX_ATTEMPTS = 2; // one retry on goto/load failure before recording NOT RUN
for (const s of sites) {
  let rec = null;
  for (let attempt = 1; attempt <= MAX_ATTEMPTS; attempt++) {

  const ctx = await browser.newContext({ viewport: list.viewport, deviceScaleFactor: 1, locale: 'en-US', timezoneId: 'America/New_York' });
  await ctx.addInitScript(IO_HOOK);
  const page = await ctx.newPage();
  const jsBundles = []; // { url, bytes }
  const responseHeaders = [];
  page.on('response', async (resp) => {
    try {
      const req = resp.request();
      if (req.resourceType() === 'script' || isJsResource(resp.url(), resp.headers())) {
        let bytes = 0;
        const cl = resp.headers()['content-length'];
        if (cl && !isNaN(parseInt(cl, 10))) bytes = parseInt(cl, 10);
        else {
          try {
            const buf = await resp.body();
            bytes = buf ? buf.length : 0;
          } catch { /* body may be unavailable */ }
        }
        jsBundles.push({ url: resp.url().slice(0, 200), bytes });
      }
      if (resp.request().isNavigationRequest() || resp.url() === s.url || resp.url().startsWith(s.url)) {
        responseHeaders.push({ url: resp.url().slice(0, 200), status: resp.status(), headers: resp.headers() });
      }
    } catch { /* ignore */ }
  });
  rec = { id: s.id, url: s.url, status: 'NOT RUN', reason: null };
  const t0 = Date.now();
  try {
    const resp = await page.goto(s.url, { waitUntil: 'domcontentloaded', timeout: 30000 });
    rec.http_status = resp ? resp.status() : null;
    const mainHeaders = resp ? resp.headers() : {};
    await page.waitForLoadState('networkidle', { timeout: 8000 }).catch(() => {});
    await page.waitForTimeout(1500);
    rec.data = await page.evaluate(`(${classifier})()`);

    // JS bundle summary
    const byUrl = new Map();
    for (const b of jsBundles) {
      const prev = byUrl.get(b.url);
      if (!prev || b.bytes > prev.bytes) byUrl.set(b.url, b);
    }
    const bundles = [...byUrl.values()];
    const totalBytes = bundles.reduce((a, b) => a + (b.bytes || 0), 0);
    const over1mb = bundles.filter(b => b.bytes >= 1024 * 1024);
    rec.data.bundles = {
      count: bundles.length,
      total_bytes: totalBytes,
      total_mb: Math.round(totalBytes / 1024 / 1024 * 100) / 100,
      over_1mb_count: over1mb.length,
      over_1mb: over1mb.slice(0, 10).map(b => ({ url: b.url, mb: Math.round(b.bytes / 1024 / 1024 * 100) / 100 })),
    };

    // Framework extras from script URLs
    const scriptUrls = bundles.map(b => b.url).join(' ');
    const fw = rec.data.frameworks || { detected: [], signals: {} };
    const addFw = (name, signal) => {
      if (!fw.detected.includes(name)) fw.detected.push(name);
      if (!fw.signals[name]) fw.signals[name] = [];
      if (fw.signals[name].length < 5) fw.signals[name].push(signal);
    };
    if (/\/_next\//.test(scriptUrls)) addFw('Next.js', 'script:/_next/');
    if (/react(-dom)?(\.min)?\.js|react\.production/i.test(scriptUrls)) addFw('React', 'script:react');
    if (/vue(\.runtime)?(\.min)?\.js|vue\.global/i.test(scriptUrls)) addFw('Vue', 'script:vue');
    if (/angular/i.test(scriptUrls)) addFw('Angular', 'script:angular');
    if (/cdn\.shopify\.com/i.test(scriptUrls)) addFw('Shopify', 'script:shopify');
    rec.data.frameworks = fw;

    // Bot wall merge — exclusive status: HTTP client/error challenge codes are BOTWALL, not OK.
    const hdrBw = classifyBotWallFromHeaders(mainHeaders, rec.http_status);
    const pageBw = rec.data.botWall || { vendors: [], signals: [] };
    const vendors = [...new Set([...(pageBw.vendors || []), ...hdrBw.vendors])];
    const titleBlob = `${rec.data.title || ''} ${(pageBw.signals || []).join(' ')}`;
    const titleChallenge = /just a moment|access denied|attention required|captcha|challenge|bot or not|security checkpoint|are you a human|verify you are/i.test(titleBlob);
    const httpWall = [401, 403, 429, 503].includes(rec.http_status);
    const isChallenge = httpWall || titleChallenge || (pageBw.vendors && pageBw.vendors.length > 0 && titleChallenge);
    rec.data.botWall = {
      vendors,
      signals: [...new Set([...(pageBw.signals || []), ...hdrBw.signals, ...(httpWall ? [`http-${rec.http_status}`] : []), ...(titleChallenge ? ['title-challenge'] : [])])],
      http_status: rec.http_status,
      is_challenge: !!isChallenge,
    };
    if (isChallenge) rec.status = 'BOTWALL';
    else rec.status = 'OK';
  } catch (e) {
    rec.reason = String(e && e.message || e).split('\n')[0].slice(0, 200);
  }
  rec.ms = Date.now() - t0;
  const tag = rec.status.padEnd(8);
  const bw = rec.data && rec.data.botWall && rec.data.botWall.vendors.length ? ` bw=${rec.data.botWall.vendors.join('+')}` : '';
  const bun = rec.data && rec.data.bundles ? ` js=${rec.data.bundles.count}/${rec.data.bundles.total_mb}MB/>1MB=${rec.data.bundles.over_1mb_count}` : '';
  console.error(`${tag} ${s.id} ${rec.ms}ms http=${rec.http_status || '-'}${bw}${bun} attempt=${attempt} ${rec.reason || ''}`);
  await ctx.close();
    // One retry only when the load never produced a page record (timeout / net error).
    if (rec.status !== 'NOT RUN' || attempt === MAX_ATTEMPTS) break;
    console.error(`RETRY    ${s.id} after fail: ${rec.reason}`);
  }
  results.push(rec);
}
await browser.close();
meta.finished_utc = new Date().toISOString();
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, JSON.stringify({ meta, results }, null, 1));
console.error(`wrote ${outPath}`);
