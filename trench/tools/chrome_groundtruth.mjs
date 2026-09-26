#!/usr/bin/env node
// chrome_groundtruth.mjs — what pinned Chrome actually does to load a real site.
//
// Per URL, with the oracle's deterministic launch (tools/parity_oracle/deterministic.mjs),
// 1280x800, load + settle:
//   - performance trace, summarised as main-thread self time per phase
//     (parse / script / style / layout / paint / gc), with style element counts,
//     layout count + dirty objects and GC counts;
//   - library census (frameworks, tag managers, polyfills, custom elements, shadow roots);
//   - JS coverage (function granularity) and CSS rule usage, at first contentful
//     paint and at load + settle;
//   - counts of selected Web API calls made during load (an init script wraps them);
//   - V8 heap used at the end.
//
//   PARITY_CHROME_PATH=<pinned CfT> node trench/tools/chrome_groundtruth.mjs \
//       --sites websuite/realsite-top20.json [--extra id=url ...] --out <dir> [--settle-ms 5000]
//
// Playwright is resolved from tools/parity_oracle (run `npm ci` there), or from
// the package.json named by PW_ROOT. Writes <out>/<id>.json per site, <out>/summary.json,
// and gzipped raw traces to <out>/traces/ (large; never commit them).

import { createRequire } from 'module';
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync } from 'fs';
import { gzipSync } from 'zlib';
import { dirname, resolve, join } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, '../..');
const require = createRequire(process.env.PW_ROOT || join(REPO, 'tools/parity_oracle/package.json'));
const { chromium } = require('playwright');
const { getDeterministicLaunchOptions } = await import(join(REPO, 'tools/parity_oracle/deterministic.mjs'));

// ---------- args ----------
const argv = process.argv.slice(2);
const opt = { sites: null, extra: [], out: null, settle: 5000, only: [], blocked: [], resume: false, siteTimeout: 150 };
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (a === '--sites') opt.sites = argv[++i];
  else if (a === '--extra') opt.extra.push(argv[++i]);
  else if (a === '--out') opt.out = argv[++i];
  else if (a === '--settle-ms') opt.settle = parseInt(argv[++i], 10);
  else if (a === '--only') opt.only.push(argv[++i]);
  else if (a === '--blocked') opt.blocked.push(...argv[++i].split(','));
  else if (a === '--resume') opt.resume = true;
  else if (a === '--site-timeout-s') opt.siteTimeout = parseInt(argv[++i], 10);
}
if (!opt.out) { console.error('--out <dir> required'); process.exit(2); }
let sites = [];
if (opt.sites) sites = JSON.parse(readFileSync(opt.sites, 'utf8')).sites.map(s => ({ id: s.id, url: s.url }));
for (const e of opt.extra) { const k = e.indexOf('='); sites.push({ id: e.slice(0, k), url: e.slice(k + 1) }); }
if (opt.only.length) sites = sites.filter(s => opt.only.includes(s.id));
mkdirSync(join(opt.out, 'traces'), { recursive: true });

// ---------- API counters (init script) ----------
const API_INIT = `(() => {
  const C = Object.create(null);
  Object.defineProperty(window, '__gtApi', { value: C, enumerable: false });
  const bump = k => { C[k] = (C[k] || 0) + 1; };
  const wrapFn = (obj, prop, key) => {
    try {
      const orig = obj && obj[prop];
      if (typeof orig !== 'function') return;
      const w = function (...a) { bump(key); return orig.apply(this, a); };
      Object.defineProperty(w, 'name', { value: orig.name });
      obj[prop] = w;
    } catch (e) {}
  };
  const wrapCtor = (obj, prop, key) => {
    try {
      const orig = obj && obj[prop];
      if (typeof orig !== 'function') return;
      obj[prop] = new Proxy(orig, {
        construct(t, a, nt) { bump(key); return Reflect.construct(t, a, nt); },
        apply(t, th, a) { bump(key); return Reflect.apply(t, th, a); },
      });
    } catch (e) {}
  };
  const wrapGetter = (proto, prop, key) => {
    try {
      const d = Object.getOwnPropertyDescriptor(proto, prop);
      if (!d || !d.get) return;
      Object.defineProperty(proto, prop, { ...d, get() { bump(key); return d.get.call(this); } });
    } catch (e) {}
  };
  wrapFn(window, 'fetch', 'fetch');
  wrapFn(XMLHttpRequest.prototype, 'open', 'XMLHttpRequest');
  for (const k of ['IntersectionObserver', 'ResizeObserver', 'MutationObserver', 'Promise',
                   'WeakMap', 'WeakSet', 'WeakRef', 'FinalizationRegistry', 'Proxy'])
    wrapCtor(window, k, k);
  wrapFn(window, 'requestAnimationFrame', 'requestAnimationFrame');
  wrapFn(window, 'requestIdleCallback', 'requestIdleCallback');
  if (window.customElements) wrapFn(CustomElementRegistry.prototype, 'define', 'customElements.define');
  wrapFn(Element.prototype, 'attachShadow', 'attachShadow');
  wrapGetter(Window.prototype, 'localStorage', 'localStorage');
  wrapFn(window, 'matchMedia', 'matchMedia');
  wrapFn(window, 'getComputedStyle', 'getComputedStyle');
  wrapFn(Element.prototype, 'getBoundingClientRect', 'getBoundingClientRect');
  if (window.Intl) for (const k of Object.getOwnPropertyNames(Intl))
    if (typeof Intl[k] === 'function' && /^[A-Z]/.test(k)) wrapCtor(Intl, k, 'Intl.' + k);
  wrapFn(window, 'structuredClone', 'structuredClone');
  if (window.Crypto) wrapFn(Crypto.prototype, 'randomUUID', 'crypto.randomUUID');
  if (window.CSS) wrapFn(CSS, 'supports', 'CSS.supports');
  wrapGetter(Document.prototype, 'fonts', 'document.fonts');
})();`;

// ---------- census (evaluated at the end) ----------
const CENSUS = () => {
  const w = window, d = document;
  const all = d.querySelectorAll('*');
  let reactFiber = false, vueEl = false, shadowRoots = 0, svelte = false;
  const customTags = new Set(), definedTags = new Set();
  for (const el of all) {
    if (!reactFiber) for (const k in el) { if (k.startsWith('__reactFiber') || k.startsWith('__reactContainer') || k.startsWith('_reactRootContainer')) { reactFiber = true; break; } }
    if (!vueEl && (el.__vue__ || el.__vue_app__ || el.__vueParentComponent)) vueEl = true;
    if (el.shadowRoot) shadowRoots++;
    const t = el.localName;
    if (t.includes('-')) { customTags.add(t); if (w.customElements && customElements.get(t)) definedTags.add(t); }
    if (!svelte && typeof el.className === 'string' && /\bsvelte-[a-z0-9]+\b/.test(el.className)) svelte = true;
  }
  const scripts = [...d.scripts].map(s => s.src).filter(Boolean);
  const hasSrc = re => scripts.some(u => re.test(u));
  return {
    title: d.title,
    nodes: all.length,
    react: !!(w.React || reactFiber),
    next: !!(w.__NEXT_DATA__ || w.next || d.getElementById('__NEXT_DATA__')),
    vue: !!(w.Vue || w.__VUE__ || vueEl),
    nuxt: !!(w.__NUXT__ || w.$nuxt),
    angular: !!(d.querySelector('[ng-version]') || w.ng || w.getAllAngularRootElements),
    angular_version: d.querySelector('[ng-version]')?.getAttribute('ng-version') || null,
    svelte,
    jquery: (w.jQuery && w.jQuery.fn && w.jQuery.fn.jquery) || null,
    lit: !!(w.litHtmlVersions || w.litElementVersions || w.litHtmlPolyfillSupport),
    custom_element_tags: customTags.size,
    custom_elements_defined: definedTags.size,
    shadow_roots_open: shadowRoots,
    wc_polyfill: !!(w.WebComponents || w.ShadyDOM || w.ShadyCSS),
    core_js: !!(w['__core-js_shared__'] || w.core) || hasSrc(/core-js|polyfill/i),
    gtm: !!w.google_tag_manager || hasSrc(/googletagmanager\.com\/gtm/),
    gtag: typeof w.gtag === 'function' || hasSrc(/googletagmanager\.com\/gtag/),
    ga: !!(w.ga || w.GoogleAnalyticsObject) || hasSrc(/google-analytics\.com/),
    segment: !!(w.analytics && w.analytics.initialize) || hasSrc(/segment\.(com|io)/),
    optimizely: !!w.optimizely || hasSrc(/optimizely/),
    adobe_launch: !!w._satellite || hasSrc(/adobedtm|launch-/),
    ads: hasSrc(/doubleclick|googlesyndication|amazon-adsystem|adnxs|criteo|taboola|outbrain/),
    script_src_count: scripts.length,
    inline_script_count: [...d.scripts].filter(s => !s.src).length,
    stylesheet_count: d.styleSheets.length,
    api: w.__gtApi ? { ...w.__gtApi } : {},
  };
};

// ---------- trace summary ----------
const CAT = (() => {
  const m = new Map();
  const add = (cat, names) => names.forEach(n => m.set(n, cat));
  add('script', ['EvaluateScript', 'FunctionCall', 'v8.compile', 'v8.compileModule', 'V8.CompileCode',
    'v8.evaluateModule', 'TimerFire', 'EventDispatch', 'RunMicrotasks', 'v8.run', 'FireAnimationFrame',
    'FireIdleCallback', 'XHRReadyStateChange', 'XHRLoad', 'v8.callFunction', 'V8.Execute',
    'v8.produceCache', 'v8.deserializeOnBackground', 'CompileScript', 'CompileCode', 'EvaluateModule']);
  add('parse', ['ParseHTML', 'ParseAuthorStyleSheet']);
  add('style', ['UpdateLayoutTree', 'RecalculateStyles', 'ScheduleStyleRecalculation']);
  add('layout', ['Layout', 'UpdateLayerTree']);
  add('paint', ['Paint', 'PaintImage', 'PrePaint', 'Layerize', 'CompositeLayers', 'Commit', 'RasterTask', 'Decode Image', 'ImageDecodeTask']);
  add('gc', ['MinorGC', 'MajorGC', 'BlinkGC.AtomicPhase', 'V8.GCScavenger', 'V8.GCCompactor', 'V8.GCFinalizeMC',
    'V8.GC_MC_BACKGROUND_MARKING', 'V8.GCIncrementalMarking', 'V8.GCFinalizeMCReduceMemory', 'MinorMS', 'BlinkGC.IncrementalMarkingStep']);
  return m;
})();
const catOf = n => {
  const c = CAT.get(n);
  if (c) return c;
  if (n.startsWith('V8.GC') || n.startsWith('BlinkGC') || n.includes('GC_')) return 'gc';
  if (n.startsWith('v8.') || n.startsWith('V8.') || n.startsWith('LocalWindowProxy')) return 'script';
  if (n.startsWith('CSSParser') || n.startsWith('HTMLPreloadScanner') || n.startsWith('HTMLDocumentParser')) return 'parse';
  if (n.startsWith('Document::recalcStyle') || n.startsWith('StyleEngine') || n.startsWith('StyleResolver')) return 'style';
  if (n.startsWith('LocalFrameView::performLayout') || n.startsWith('LayoutNG') || n.startsWith('LayoutView')) return 'layout';
  if (n.includes('Paint') || n.startsWith('cc::') || n.startsWith('Raster')) return 'paint';
  return null;
};

function summariseTrace(events) {
  // Main thread: the CrRendererMain thread carrying the most complete-event time.
  const names = new Map();
  for (const e of events) if (e.ph === 'M' && e.name === 'thread_name') names.set(`${e.pid}:${e.tid}`, e.args?.name);
  const busy = new Map();
  for (const e of events) if (e.ph === 'X' && e.dur) {
    const k = `${e.pid}:${e.tid}`;
    if (names.get(k) === 'CrRendererMain') busy.set(k, (busy.get(k) || 0) + e.dur);
  }
  const main = [...busy.entries()].sort((a, b) => b[1] - a[1])[0]?.[0];
  if (!main) return null;
  const xs = events.filter(e => e.ph === 'X' && e.dur != null && `${e.pid}:${e.tid}` === main)
    .sort((a, b) => a.ts - b.ts || b.dur - a.dur);
  // `other` is mostly RunTask self time: scheduler work and uninstrumented Blink
  // (in this headless/SwiftShader setup, largely raster/commit waits). Not attributable.
  const self = { parse: 0, script: 0, style: 0, layout: 0, paint: 0, gc: 0, other: 0 };
  const counts = { layout: 0, layout_dirty_objects: 0, style_recalcs: 0, style_elements: 0, minor_gc: 0, major_gc: 0 };
  let total = 0;
  const stack = []; // {end, cat, childDur, dur}
  const close = f => {
    const ex = Math.max(0, f.dur - f.childDur);
    self[f.cat || 'other'] += ex;
  };
  for (const e of xs) {
    while (stack.length && stack[stack.length - 1].end <= e.ts) close(stack.pop());
    const parent = stack[stack.length - 1];
    const c = catOf(e.name) || (parent ? parent.cat : null);
    if (parent) parent.childDur += e.dur; else total += e.dur;
    stack.push({ end: e.ts + e.dur, cat: c, childDur: 0, dur: e.dur });
    if (e.name === 'Layout') { counts.layout++; counts.layout_dirty_objects += e.args?.beginData?.dirtyObjects || 0; }
    if (e.name === 'UpdateLayoutTree') { counts.style_recalcs++; counts.style_elements += e.args?.elementCount || 0; }
    if (e.name === 'MinorGC' || e.name === 'MinorMS') counts.minor_gc++;
    if (e.name === 'MajorGC') counts.major_gc++;
  }
  while (stack.length) close(stack.pop());
  const ms = v => +(v / 1000).toFixed(1);
  return {
    main_thread_busy_ms: ms(total),
    self_ms: Object.fromEntries(Object.entries(self).map(([k, v]) => [k, ms(v)])),
    ...counts,
  };
}

// ---------- coverage helpers ----------
function executedBytes(scripts, cov) {
  // Function-granularity coverage: a count-0 range marks a function that never ran.
  let total = 0, exec = 0;
  for (const s of cov) {
    const len = scripts.get(s.scriptId)?.length;
    if (!len) continue;
    const zero = [];
    for (const f of s.functions) for (const r of f.ranges) if (r.count === 0) zero.push([r.startOffset, r.endOffset]);
    zero.sort((a, b) => a[0] - b[0]);
    let dead = 0, cs = -1, ce = -1;
    for (const [a, b] of zero) {
      if (a > ce) { if (ce > cs) dead += ce - cs; cs = a; ce = b; } else ce = Math.max(ce, b);
    }
    if (ce > cs) dead += ce - cs;
    // a script whose top-level function never ran counts as fully dead
    const top = s.functions.find(f => f.ranges[0]?.startOffset === 0 && f.ranges[0]?.endOffset >= len - 1);
    const ran = top ? top.ranges[0].count > 0 : true;
    total += len; exec += ran ? Math.max(0, len - dead) : 0;
  }
  return { total, exec };
}

// ---------- clean timing pass (no tracing, coverage or init script) ----------
// Instrumentation (precise coverage, disabled-by-default trace categories) slows
// V8 and Blink, so load/FCP are also taken from one uninstrumented load.
async function cleanTiming(browser, site) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  const r = {};
  try {
    const nav = await page.goto(site.url, { waitUntil: 'load', timeout: 45000 });
    r.http_status = nav ? nav.status() : null;
    await page.waitForTimeout(1500);
    Object.assign(r, await page.evaluate(() => {
      const n = performance.getEntriesByType('navigation')[0];
      const fcp = performance.getEntriesByName('first-contentful-paint')[0];
      return { load_ms: n ? Math.round(n.loadEventEnd) : null, fcp_ms: fcp ? Math.round(fcp.startTime) : null,
               title: document.title };
    }));
  } catch (e) { r.error = String(e.message).slice(0, 160); }
  await ctx.close();
  return r;
}

// ---------- trace pass (tracing only: no coverage, no rule-usage tracking, no init script) ----------
// CSS rule-usage tracking and precise coverage slow style recalc and V8 by large
// factors, so phase timings come from a separate, trace-only load.
async function tracePass(browser, site) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  const page = await ctx.newPage();
  await browser.startTracing(page, { categories: ['devtools.timeline', 'v8', 'blink', 'loading',
    'disabled-by-default-devtools.timeline', 'blink.user_timing'] });
  let status = 'ok';
  try { await page.goto(site.url, { waitUntil: 'load', timeout: 45000 }); } catch (e) { status = 'load-timeout'; }
  await page.waitForTimeout(opt.settle);
  const buf = await browser.stopTracing();
  await ctx.close();
  writeFileSync(join(opt.out, 'traces', `${site.id}.json.gz`), gzipSync(buf));
  let trace;
  try { trace = summariseTrace(JSON.parse(buf.toString()).traceEvents || []); } catch (e) { trace = { error: String(e.message).slice(0, 120) }; }
  return { status, ...trace };
}

// ---------- per site ----------
async function probe(browser, site) {
  const ctx = await browser.newContext({ viewport: { width: 1280, height: 800 }, deviceScaleFactor: 1 });
  await ctx.addInitScript({ content: API_INIT });
  const page = await ctx.newPage();
  const cdp = await ctx.newCDPSession(page);
  const scripts = new Map();
  cdp.on('Debugger.scriptParsed', e => scripts.set(e.scriptId, { url: e.url, length: e.length }));
  await cdp.send('Debugger.enable');
  await cdp.send('Profiler.enable');
  await cdp.send('Profiler.startPreciseCoverage', { callCount: false, detailed: false });
  const sheets = new Map();
  cdp.on('CSS.styleSheetAdded', e => sheets.set(e.header.styleSheetId, e.header.length || 0));
  await cdp.send('DOM.enable');
  await cdp.send('CSS.enable');
  await cdp.send('CSS.startRuleUsageTracking');
  await cdp.send('Performance.enable');

  const out = { id: site.id, url: site.url, status: 'ok' };
  let fcpJs = null, fcpCssUsed = new Map();
  const jsUnion = new Map();           // scriptId -> merged coverage snapshots
  const takeJs = async () => {
    const { result } = await cdp.send('Profiler.takePreciseCoverage');
    for (const s of result) {
      const prev = jsUnion.get(s.scriptId);
      if (!prev) { jsUnion.set(s.scriptId, s); continue; }
      // union by function: a function executed in either snapshot counts as executed
      const byKey = new Map(prev.functions.map(f => [`${f.ranges[0].startOffset}:${f.ranges[0].endOffset}`, f]));
      for (const f of s.functions) {
        const k = `${f.ranges[0].startOffset}:${f.ranges[0].endOffset}`;
        const p = byKey.get(k);
        if (!p) { prev.functions.push(f); continue; }
        p.ranges = p.ranges.map((r, i) => ({ ...r, count: Math.max(r.count, f.ranges[i]?.count || 0) }));
      }
    }
  };
  const cssUsed = new Map();         // `${sheet}:${start}` -> rule bytes
  const takeCss = async () => {
    const { coverage } = await cdp.send('CSS.takeCoverageDelta');
    for (const r of coverage) if (r.used) cssUsed.set(`${r.styleSheetId}:${r.startOffset}`, r.endOffset - r.startOffset);
  };
  const usedBytes = m => [...m.values()].reduce((a, b) => a + b, 0);

  const t0 = Date.now();
  let nav;
  try { nav = await page.goto(site.url, { waitUntil: 'commit', timeout: 45000 }); }
  catch (e) { out.status = 'nav-error'; out.error = String(e.message).slice(0, 200); }
  out.http_status = nav ? nav.status() : null;
  // Poll for first contentful paint, and snapshot coverage when it lands.
  const deadline = Date.now() + 45000;
  let loadedAt = null;
  page.waitForLoadState('load', { timeout: 45000 }).then(() => { loadedAt = Date.now(); }).catch(() => {});
  // FCP can land after `load` (late web fonts, hydration), so keep polling
  // through the settle window; the FCP snapshot is taken the moment it appears.
  while (Date.now() < deadline) {
    if (!fcpJs) {
      let fcp = null;
      try { fcp = await page.evaluate(() => performance.getEntriesByName('first-contentful-paint')[0]?.startTime ?? null); } catch (e) {}
      if (fcp != null) {
        await takeJs(); await takeCss();
        fcpJs = executedBytes(scripts, [...jsUnion.values()]);
        fcpCssUsed = new Map(cssUsed);
      }
    }
    if (loadedAt && Date.now() - loadedAt >= opt.settle) break;
    await page.waitForTimeout(100);
  }
  if (!loadedAt && out.status === 'ok') out.status = 'load-timeout';

  await takeJs(); await takeCss();
  try {
    const { ruleUsage } = await cdp.send('CSS.stopRuleUsageTracking');
    for (const r of ruleUsage) if (r.used) cssUsed.set(`${r.styleSheetId}:${r.startOffset}`, r.endOffset - r.startOffset);
  } catch (e) {}
  const cssBytes = [...sheets.values()].reduce((a, b) => a + b, 0);
  const endJs = executedBytes(scripts, [...jsUnion.values()]);
  const metrics = Object.fromEntries((await cdp.send('Performance.getMetrics')).metrics.map(m => [m.name, m.value]));
  let timing = {};
  try {
    timing = await page.evaluate(() => {
      const n = performance.getEntriesByType('navigation')[0];
      const fcp = performance.getEntriesByName('first-contentful-paint')[0];
      return { load_ms: n ? Math.round(n.loadEventEnd) : null, dcl_ms: n ? Math.round(n.domContentLoadedEventEnd) : null,
               fcp_ms: fcp ? Math.round(fcp.startTime) : null };
    });
  } catch (e) {}
  let census = {};
  try { census = await page.evaluate(CENSUS); } catch (e) { census = { error: String(e.message).slice(0, 120) }; }


  const pct = (a, b) => (b ? +(100 * a / b).toFixed(1) : null);
  Object.assign(out, {
    wall_ms: Date.now() - t0,
    timing_instrumented: timing,
    js: {
      scripts: scripts.size,
      bytes_total: endJs.total,
      bytes_exec_at_fcp: fcpJs ? fcpJs.exec : null,
      bytes_exec_at_end: endJs.exec,
      pct_exec_at_fcp: fcpJs ? pct(fcpJs.exec, endJs.total) : null,
      pct_exec_at_end: pct(endJs.exec, endJs.total),
    },
    css: {
      // Rule-usage coverage, by bytes of rule text (as DevTools' Coverage panel reports it).
      sheets: sheets.size,
      bytes_total: cssBytes,
      rules_used_at_fcp: fcpCssUsed.size,
      rules_used_at_end: cssUsed.size,
      pct_bytes_used_at_fcp: pct(usedBytes(fcpCssUsed), cssBytes),
      pct_bytes_used_at_end: pct(usedBytes(cssUsed), cssBytes),
    },
    heap: { js_heap_used_mb: +((metrics.JSHeapUsedSize || 0) / 1048576).toFixed(1),
            js_heap_total_mb: +((metrics.JSHeapTotalSize || 0) / 1048576).toFixed(1),
            dom_nodes: metrics.Nodes, layout_count: metrics.LayoutCount, recalc_style_count: metrics.RecalcStyleCount },
    census,
  });
  await ctx.close();
  return out;
}

// One browser per site: a runaway page (seen: GPU process at ~600% CPU for 15+ min)
// is contained by closing, and if need be killing, that site's browser.
const withTimeout = (p, ms, label) => Promise.race([p, new Promise((_, rej) => setTimeout(() => rej(new Error(`timeout: ${label} > ${ms} ms`)), ms))]);
for (const s of sites) {
  const file = join(opt.out, `${s.id}.json`);
  if (opt.resume && existsSync(file)) {
    try { const prev = JSON.parse(readFileSync(file, 'utf8')); if (prev.status === 'ok' || prev.status === 'blocked') { process.stderr.write(`[groundtruth] ${s.id} (resume: kept)\n`); continue; } } catch (e) {}
  }
  if (opt.blocked.includes(s.id)) {
    writeFileSync(file, JSON.stringify({ id: s.id, url: s.url, status: 'blocked', note: 'oracle blocked or challenged from this Mac; not probed' }, null, 2));
    process.stderr.write(`[groundtruth] ${s.id} blocked (skipped)\n`);
    continue;
  }
  process.stderr.write(`[groundtruth] ${s.id} ${s.url}\n`);
  const browser = await chromium.launch(getDeterministicLaunchOptions());
  const pid = browser.process?.()?.pid;
  const r = { id: s.id, url: s.url };
  const budget = opt.siteTimeout * 1000;
  const t0 = Date.now();
  const left = () => Math.max(5000, budget - (Date.now() - t0));
  try {
    Object.assign(r, await withTimeout(probe(browser, s), left(), 'coverage pass'));
    r.clean = await withTimeout(cleanTiming(browser, s), left(), 'clean pass');
    r.trace = await withTimeout(tracePass(browser, s), left(), 'trace pass');
  } catch (e) {
    r.status = /^timeout/.test(e.message) ? 'timeout' : 'probe-error';
    r.error = String(e.message).slice(0, 300);
  }
  await withTimeout(browser.close(), 10000, 'close').catch(() => { if (pid) try { process.kill(pid, 'SIGKILL'); } catch (e) {} });
  writeFileSync(file, JSON.stringify(r, null, 2));
  process.stderr.write(`  -> ${r.status} clean_load=${r.clean?.load_ms} clean_fcp=${r.clean?.fcp_ms} busy=${r.trace?.main_thread_busy_ms} js_exec=${r.js?.pct_exec_at_end}% css_used=${r.css?.pct_bytes_used_at_end}%\n`);
}
const results = sites.map(s => { try { return JSON.parse(readFileSync(join(opt.out, `${s.id}.json`), 'utf8')); } catch (e) { return { id: s.id, url: s.url, status: 'missing' }; } });
writeFileSync(join(opt.out, 'summary.json'), JSON.stringify({ ts: new Date().toISOString(), settle_ms: opt.settle, results }, null, 2));
