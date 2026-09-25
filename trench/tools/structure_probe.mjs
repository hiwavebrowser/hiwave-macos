/**
 * structure_probe.mjs — Chrome-only page-structure inventory for the real-site board.
 *
 *   PARITY_CHROME_PATH=<pinned CfT> node trench/tools/structure_probe.mjs <out.json> [site-id ...]
 *
 * For each site in websuite/realsite-top20.json (or the ids given) it loads the
 * URL twice in pinned Chrome at 1280x800, light scheme, logged out:
 *   1. JS on: DOM/tag counts, computed display/position mix (whole page and
 *      first viewport), images and their served formats, media/embeds,
 *      custom elements/shadow roots, stylesheet + inline <style> text scanned
 *      for at-rules and modern CSS features, script count/bytes, web fonts.
 *   2. JS off: first-viewport text, to measure how much of what Chrome shows
 *      exists without running any script (server HTML share).
 * It records what the page USES, not whether RustKit supports it; the
 * analysis joins the two. One pass per site, sequential, no RustKit.
 */
import { readFileSync, writeFileSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '../..');
const { chromium } = await import(resolve(REPO, 'tools/parity_oracle/node_modules/playwright/index.mjs'));
const { getDeterministicLaunchOptions } = await import(resolve(REPO, 'tools/parity_oracle/deterministic.mjs'));

const W = 1280, H = 800, SETTLE_MS = 5000, NAV_TIMEOUT_MS = 30000;

// Feature patterns scanned over all CSS text (external sheets + inline <style>).
const CSS_FEATURES = {
  media: /@media\b/g, supports: /@supports\b/g, container_rule: /@container\b/g, layer: /@layer\b/g,
  font_face: /@font-face\b/g, keyframes: /@keyframes\b/g, import: /@import\b/g, property: /@property\b/g,
  scope: /@scope\b/g, starting_style: /@starting-style\b/g,
  has: /:has\(/g, is: /:is\(/g, where: /:where\(/g, not_complex: /:not\([^)]*[\s>,]/g, focus_visible: /:focus-visible/g,
  nesting_amp: /&[\s.:#\[>+~]/g,
  var: /var\(--/g, custom_prop_decl: /--[\w-]+\s*:/g, calc: /calc\(/g, clamp: /clamp\(/g, min_max: /\b(?:min|max)\(/g,
  color_mix: /color-mix\(/g, oklch_lab: /\b(?:ok)?(?:lch|lab)\(/g, light_dark: /light-dark\(/g,
  aspect_ratio: /aspect-ratio\s*:/g, container_type: /container-type\s*:/g, grid_template_areas: /grid-template-areas\s*:/g,
  subgrid: /subgrid/g, sticky: /position\s*:\s*(?:-webkit-)?sticky/g, inset: /\binset\s*:/g, gap: /\bgap\s*:/g,
  backdrop_filter: /backdrop-filter\s*:/g, filter: /[^-]filter\s*:/g, mask: /mask(?:-image)?\s*:/g, clip_path: /clip-path\s*:/g,
  transform: /transform\s*:/g, transition: /transition\s*:/g, animation: /animation(?:-name)?\s*:/g,
  object_fit: /object-fit\s*:/g, text_wrap: /text-wrap\s*:/g, line_clamp: /line-clamp\s*:/g, writing_mode: /writing-mode\s*:/g,
  scroll_snap: /scroll-snap-type\s*:/g, display_contents: /display\s*:\s*contents/g, content_visibility: /content-visibility\s*:/g,
  gradient: /(?:linear|radial|conic)-gradient\(/g, image_set: /image-set\(/g, view_transition: /view-transition/g,
};

function inventoryInPage() {
  const Wv = window.innerWidth, Hv = window.innerHeight;
  const all = Array.from(document.getElementsByTagName('*'));
  const tags = {};
  const disp = {}, vdisp = {}, pos = {}, vpos = {};
  const feat = { transform: 0, filter: 0, backdrop: 0, opacity_lt1: 0, aspect_ratio: 0, object_fit: 0, sticky: 0,
    gradient_bg: 0, url_bg: 0, multi_bg: 0, border_radius: 0, box_shadow: 0, clip_path: 0, mask: 0, float: 0,
    ellipsis: 0, flex_gap: 0, grid_areas: 0, overflow_clip: 0, z_index: 0, custom_elements: 0, shadow_roots: 0 };
  const vfeat = { flex: 0, grid: 0, abs_fixed: 0, transform: 0, gradient_bg: 0, url_bg: 0, img: 0, svg: 0, text_elems: 0 };
  let inViewport = 0;
  for (const el of all) {
    const t = el.tagName.toLowerCase();
    tags[t] = (tags[t] || 0) + 1;
    if (t.includes('-')) feat.custom_elements++;
    if (el.shadowRoot) feat.shadow_roots++;
    const cs = getComputedStyle(el);
    const d = cs.display, p = cs.position;
    disp[d] = (disp[d] || 0) + 1;
    pos[p] = (pos[p] || 0) + 1;
    if (cs.transform !== 'none') feat.transform++;
    if (cs.filter !== 'none') feat.filter++;
    if (cs.backdropFilter && cs.backdropFilter !== 'none') feat.backdrop++;
    if (parseFloat(cs.opacity) < 1) feat.opacity_lt1++;
    if (cs.aspectRatio && cs.aspectRatio !== 'auto') feat.aspect_ratio++;
    if (cs.objectFit && cs.objectFit !== 'fill') feat.object_fit++;
    if (p === 'sticky') feat.sticky++;
    const bg = cs.backgroundImage;
    if (bg && bg !== 'none') {
      if (/gradient\(/.test(bg)) feat.gradient_bg++;
      if (/url\(/.test(bg)) feat.url_bg++;
      if (bg.split(/,(?![^(]*\))/).length > 1) feat.multi_bg++;
    }
    if (cs.borderTopLeftRadius !== '0px') feat.border_radius++;
    if (cs.boxShadow !== 'none') feat.box_shadow++;
    if (cs.clipPath && cs.clipPath !== 'none') feat.clip_path++;
    if ((cs.maskImage || cs.webkitMaskImage || 'none') !== 'none') feat.mask++;
    if (cs.float !== 'none') feat.float++;
    if (cs.textOverflow === 'ellipsis') feat.ellipsis++;
    if ((d === 'flex' || d === 'inline-flex') && cs.columnGap !== 'normal' && cs.columnGap !== '0px') feat.flex_gap++;
    if (cs.gridTemplateAreas && cs.gridTemplateAreas !== 'none') feat.grid_areas++;
    if (cs.overflowX !== 'visible' || cs.overflowY !== 'visible') feat.overflow_clip++;
    if (cs.zIndex !== 'auto') feat.z_index++;
    if (d === 'none') continue;
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.height > 0 && r.bottom > 0 && r.right > 0 && r.top < Hv && r.left < Wv) {
      inViewport++;
      vdisp[d] = (vdisp[d] || 0) + 1;
      vpos[p] = (vpos[p] || 0) + 1;
      if (d.includes('flex')) vfeat.flex++;
      if (d.includes('grid')) vfeat.grid++;
      if (p === 'absolute' || p === 'fixed') vfeat.abs_fixed++;
      if (cs.transform !== 'none') vfeat.transform++;
      if (bg && /gradient\(/.test(bg)) vfeat.gradient_bg++;
      if (bg && /url\(/.test(bg)) vfeat.url_bg++;
      if (t === 'img') vfeat.img++;
      if (t === 'svg') vfeat.svg++;
      for (const n of el.childNodes) if (n.nodeType === 3 && n.nodeValue.trim()) { vfeat.text_elems++; break; }
    }
  }
  const imgs = Array.from(document.images);
  const images = {
    img: imgs.length, lazy: imgs.filter(i => i.loading === 'lazy').length,
    srcset: imgs.filter(i => i.srcset).length, picture: document.getElementsByTagName('picture').length,
    inline_svg: document.getElementsByTagName('svg').length,
    img_svg_src: imgs.filter(i => /\.svg(\?|$)/i.test(i.currentSrc || i.src)).length,
    data_uri: imgs.filter(i => (i.currentSrc || i.src).startsWith('data:')).length,
  };
  const media = {
    canvas: document.getElementsByTagName('canvas').length, video: document.getElementsByTagName('video').length,
    audio: document.getElementsByTagName('audio').length, iframe: document.getElementsByTagName('iframe').length,
  };
  const scripts = Array.from(document.scripts);
  const scriptInfo = {
    count: scripts.length, external: scripts.filter(s => s.src).length,
    module: scripts.filter(s => s.type === 'module').length, nomodule: scripts.filter(s => s.noModule).length,
    inline_bytes: scripts.filter(s => !s.src).reduce((a, s) => a + s.text.length, 0),
  };
  const inlineCss = Array.from(document.querySelectorAll('style')).map(s => s.textContent).join('\n');
  let accessibleRules = 0, inaccessibleSheets = 0;
  for (const sh of document.styleSheets) { try { accessibleRules += sh.cssRules.length; } catch { inaccessibleSheets++; } }
  const fonts = [];
  document.fonts.forEach(f => { if (f.status === 'loaded') fonts.push(f.family.replace(/"/g, '')); });
  return {
    dom_nodes: all.length, in_viewport_elems: inViewport,
    tags_top: Object.entries(tags).sort((a, b) => b[1] - a[1]).slice(0, 25),
    display: disp, viewport_display: vdisp, position: pos, viewport_position: vpos,
    features: feat, viewport_features: vfeat, images, media, scripts: scriptInfo,
    stylesheets: { count: document.styleSheets.length, style_elems: document.querySelectorAll('style').length,
      accessible_top_rules: accessibleRules, cross_origin_sheets: inaccessibleSheets },
    webfonts_loaded: [...new Set(fonts)],
    inline_css: inlineCss,
    doc_height: document.documentElement.scrollHeight,
  };
}

function viewportText() {
  const Wv = window.innerWidth, Hv = window.innerHeight, out = [];
  const root = document.body || document.documentElement;
  if (!root) return '';
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  const range = document.createRange();
  let n;
  while ((n = walker.nextNode())) {
    const t = n.nodeValue;
    if (!t || !t.trim()) continue;
    const el = n.parentElement;
    if (!el || el.closest('script,style,noscript,template')) continue;
    if (!el.checkVisibility({ opacityProperty: true, visibilityProperty: true })) continue;
    range.selectNodeContents(n);
    for (const r of range.getClientRects()) {
      if (r.width > 0 && r.height > 0 && r.bottom > 0 && r.right > 0 && r.top < Hv && r.left < Wv) { out.push(t); break; }
    }
  }
  return out.join(' ');
}

const words = (s) => new Set((s.match(/[\p{L}\p{N}_]+/gu) || []).map(w => w.toLowerCase()));

async function load(browser, url, js) {
  const context = await browser.newContext({ viewport: { width: W, height: H }, deviceScaleFactor: 1, colorScheme: 'light',
    locale: 'en-US', javaScriptEnabled: js, extraHTTPHeaders: { 'Sec-CH-Prefers-Color-Scheme': 'light' } });
  const page = await context.newPage();
  const res = { css: [], img_types: {}, js_bytes: 0, js_files: 0, font_files: 0, doc_bytes: null, doc_status: null, requests: 0, bytes: 0 };
  const pending = [];
  page.on('response', (r) => {
    res.requests++;
    const ct = (r.headers()['content-type'] || '').split(';')[0].trim();
    const rt = r.request().resourceType();
    const len = Number(r.headers()['content-length'] || 0);
    res.bytes += len;
    if (rt === 'image') { const k = ct || 'unknown'; res.img_types[k] = (res.img_types[k] || 0) + 1; }
    if (rt === 'font') res.font_files++;
    if (rt === 'script') { res.js_files++; pending.push(r.body().then(b => { res.js_bytes += b.length; }).catch(() => {})); }
    if (rt === 'stylesheet') pending.push(r.text().then(t => res.css.push(t)).catch(() => {}));
    if (rt === 'document' && r.request().frame() === page.mainFrame() && res.doc_status === null) {
      res.doc_status = r.status();
      pending.push(r.body().then(b => { res.doc_bytes = b.length; }).catch(() => {}));
    }
  });
  let nav_error = null;
  const t0 = Date.now();
  try { await page.goto(url, { waitUntil: 'load', timeout: NAV_TIMEOUT_MS }); }
  catch (e) { nav_error = String(e.message || e).split('\n')[0]; }
  const load_ms = Date.now() - t0;
  await page.waitForTimeout(SETTLE_MS);
  // Streaming responses (long-poll, SSE) never finish their body; cap the wait.
  await Promise.race([Promise.all(pending), new Promise(r => setTimeout(r, 10000))]);
  let inv = null, vtext = '';
  try { vtext = await page.evaluate(viewportText); if (js) inv = await page.evaluate(inventoryInPage); }
  catch (e) { nav_error = nav_error || String(e.message || e).split('\n')[0]; }
  await context.close();
  return { res, inv, vtext, nav_error, load_ms };
}

const [outPath, ...only] = process.argv.slice(2);
if (!outPath) { console.error('usage: structure_probe.mjs <out.json> [site-id ...]'); process.exit(2); }
const list = JSON.parse(readFileSync(resolve(REPO, 'websuite/realsite-top20.json'), 'utf8')).sites
  .filter(s => only.length === 0 || only.includes(s.id));
const browser = await chromium.launch(getDeterministicLaunchOptions());
const out = { ts: new Date().toISOString(), chrome: browser.version(), viewport: { width: W, height: H }, sites: {} };
for (const s of list) {
  const rec = { url: s.url };
  try {
    const on = await load(browser, s.url, true);
    const off = await load(browser, s.url, false);
    const cssText = on.res.css.join('\n') + '\n' + (on.inv ? on.inv.inline_css : '');
    const cssCounts = {};
    for (const [k, re] of Object.entries(CSS_FEATURES)) cssCounts[k] = (cssText.match(re) || []).length;
    // Declaration histogram: property names that follow '{' or ';' (custom
    // properties excluded). Joined later against the engine's parse arms.
    const props = {};
    for (const m of cssText.matchAll(/[{;]\s*(-?[a-z][a-z-]*)\s*:/g)) props[m[1]] = (props[m[1]] || 0) + 1;
    if (on.inv) delete on.inv.inline_css;
    const wOn = words(on.vtext), wOff = words(off.vtext);
    let shared = 0; for (const w of wOn) if (wOff.has(w)) shared++;
    Object.assign(rec, {
      load_ms: on.load_ms, nav_error: on.nav_error, doc_status: on.res.doc_status, doc_bytes: on.res.doc_bytes,
      requests: on.res.requests, js_files: on.res.js_files, js_bytes: on.res.js_bytes, font_files: on.res.font_files,
      css_files: on.res.css.length, css_bytes: cssText.length, css_features: cssCounts, css_properties: props, img_types: on.res.img_types,
      inventory: on.inv,
      viewport_words_js: wOn.size, viewport_words_nojs: wOff.size,
      server_html_share: wOn.size ? +(shared / wOn.size).toFixed(3) : null,
      nojs_nav_error: off.nav_error, nojs_doc_status: off.res.doc_status,
    });
  } catch (e) { rec.error = String(e.message || e).split('\n')[0]; }
  out.sites[s.id] = rec;
  writeFileSync(outPath, JSON.stringify(out, null, 1)); // incremental: a stuck site never loses the others
  console.error(`${s.id}: ${rec.error || `${rec.inventory ? rec.inventory.dom_nodes : '?'} nodes, server-html ${rec.server_html_share}`}`);
}
await browser.close();
writeFileSync(outPath, JSON.stringify(out, null, 1));
