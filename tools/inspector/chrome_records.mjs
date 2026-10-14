/**
 * chrome_records.mjs - Chromium's element diagnostics records, in the engine's shape
 *
 *   node tools/inspector/chrome_records.mjs <url|html-file> <out.json>
 *        [--width 1280] [--height 800] [--scale 1] [--settle 1500]
 *        [--max-elements 5000] [--keep-query]
 *
 * Writes the same dump `parity-capture --dump-diagnostics` writes (schema
 * hiwave-element-diagnostics/1, see crates/rustkit-engine/src/diagnostics.rs):
 * one record per element, in tree order, open shadow trees included after
 * their host. Fields script cannot know are null with a reason under
 * `unavailable`, as on the engine side.
 *
 * Page setup is element_rects.mjs's: an html file gets capture_baseline.mjs's
 * deterministic context (and parity reset where it applies), a URL gets the
 * real-site context. `oracle_key` is getSelector read out of
 * capture_baseline.mjs, as element_rects.mjs does.
 *
 * Privacy (record_scrub.mjs): the page URL loses its user name and password,
 * and its query, fragment and `;` path parameters unless --keep-query; a
 * data:, javascript: or blob: page is `data:<mime>`, `javascript:` or `blob:`
 * and its origin either way; a local page is named by a relative path, never
 * a host path. Text is reported as a length only. Every url() in
 * background-image loses userinfo, query, fragment and `;` path parameters,
 * data: URLs their contents, javascript: and blob: URLs as above, and file:
 * URLs become paths relative to the page. Error messages name the page as
 * the record does and scrub any other URL in them. Nothing reads cookies,
 * storage or form values.
 */

// Playwright is installed once, in tools/parity_oracle (`npm ci` there).
import { chromium } from '../parity_oracle/node_modules/playwright/index.mjs';
import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'fs';
import { dirname, join, resolve } from 'path';
import { fileURLToPath, pathToFileURL } from 'url';
import { backgroundImage, isUrlTarget, redactTarget, reportTarget } from './record_scrub.mjs';
import {
  createDeterministicContext,
  getDeterministicLaunchOptions,
  shouldApplyParityResetForHtmlPath,
} from '../parity_oracle/deterministic.mjs';
import { extractGetSelectorSource } from '../parity_oracle/verify_selector_key.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const NAV_TIMEOUT_MS = 30000;

// The engine's STYLE_PROPERTIES, in the same (sorted) order.
export const STYLE_PROPERTIES = [
  'align-items', 'align-self', 'aspect-ratio', 'background-color', 'background-image',
  'border-bottom-width', 'border-left-width', 'border-right-width', 'border-top-width',
  'bottom', 'box-sizing', 'clear', 'color', 'column-gap', 'contain', 'content-visibility',
  'display', 'flex-basis', 'flex-direction', 'flex-grow', 'flex-shrink', 'flex-wrap',
  'float', 'font-family', 'font-size', 'font-weight', 'gap', 'grid-auto-columns',
  'grid-auto-flow', 'grid-auto-rows', 'grid-column-end', 'grid-column-start',
  'grid-row-end', 'grid-row-start', 'grid-template-columns', 'grid-template-rows',
  'height', 'inset', 'justify-content', 'justify-self', 'left', 'line-height',
  'margin-bottom', 'margin-left', 'margin-right', 'margin-top', 'max-height', 'max-width',
  'min-height', 'min-width', 'object-fit', 'opacity', 'order', 'overflow-x', 'overflow-y',
  'padding-bottom', 'padding-left', 'padding-right', 'padding-top', 'position', 'right',
  'row-gap', 'text-align', 'top', 'transform', 'vertical-align', 'visibility',
  'white-space', 'width', 'z-index',
];

// Runs in the page.
function collect([fnSrc, props, maxElements]) {
  // eslint-disable-next-line no-new-func
  const getSelector = new Function(`${fnSrc}; return getSelector;`)();
  const SCHEMA = 'hiwave-element-diagnostics/1';
  const round = (v) => Math.round(v * 100) / 100 + 0;
  const r4 = (x, y, w, h) => ({ x: round(x), y: round(y), w: round(w), h: round(h) });

  // The capture harness's own nodes (parity-freeze.js's style) are not the
  // page's: skipped, and taken out of their ancestors' counts.
  const harness = new Set(document.querySelectorAll('style[data-parity-freeze]'));
  const harnessText = (el) => {
    let n = 0;
    for (const h of harness) if (el.contains(h)) n += (h.textContent || '').length;
    return n;
  };
  const childCount = (el) => Array.from(el.children).filter((k) => !harness.has(k)).length;
  const childIndex = (el) => {
    const p = el.parentNode;
    if (!p || !p.children) return null;
    return Array.from(p.children).filter((k) => !harness.has(k)).indexOf(el);
  };

  // Tree order, open shadow roots right after their host's own subtree start.
  const all = [];
  (function walk(root) {
    for (const el of root.children) {
      if (harness.has(el)) continue;
      all.push(el);
      if (el.shadowRoot) walk(el.shadowRoot);
      walk(el);
    }
  })(document);

  const parentOf = (el) => el.parentElement || (el.parentNode instanceof ShadowRoot ? el.parentNode.host : null);
  function structural(el) {
    const parts = [];
    for (let n = el; n; n = n.parentElement) {
      let seg = n.tagName.toLowerCase();
      const p = n.parentNode;
      if (p && p.children && childCount(p) > 1) seg += `:nth-child(${childIndex(n) + 1})`;
      parts.unshift(seg);
    }
    return parts.join(' > ');
  }
  function label(el) {
    let s = el.tagName.toLowerCase();
    if (el.id) s += `#${el.id}`;
    for (const c of el.classList) s += `.${c}`;
    return s;
  }
  const px = (v) => parseFloat(v) || 0;
  function boxes(el, cs) {
    const b = el.getBoundingClientRect();
    const bl = px(cs.borderLeftWidth), bt = px(cs.borderTopWidth);
    const br = px(cs.borderRightWidth), bb = px(cs.borderBottomWidth);
    const pl = px(cs.paddingLeft), pt = px(cs.paddingTop);
    const pr = px(cs.paddingRight), pb = px(cs.paddingBottom);
    const padding = [b.x + bl, b.y + bt, b.width - bl - br, b.height - bt - bb];
    const content = [padding[0] + pl, padding[1] + pt, padding[2] - pl - pr, padding[3] - pt - pb];
    return { border: b, padding, content };
  }
  const isFlex = (d) => d === 'flex' || d === 'inline-flex';
  const isGrid = (d) => d === 'grid' || d === 'inline-grid';
  const BLOCK_CONTAINERS = new Set(['block', 'inline-block', 'flow-root', 'list-item', 'flex', 'inline-flex',
    'grid', 'inline-grid', 'table-cell', 'table-caption', 'table', 'inline-table']);
  function establishesAbsCb(cs) {
    return cs.position !== 'static' || cs.transform !== 'none' || cs.filter !== 'none'
      || cs.perspective !== 'none' || /paint|layout|strict|content/.test(cs.contain || '');
  }
  const csOf = new Map();
  const cs = (el) => { let c = csOf.get(el); if (!c) { c = getComputedStyle(el); csOf.set(el, c); } return c; };
  const hasBox = (el) => el.getClientRects().length > 0;

  const vw = innerWidth, vh = innerHeight;
  const out = [];
  for (const el of all) {
    if (out.length >= maxElements) break;
    const c = cs(el);
    const unavailable = {};
    const notes = [];
    const parent = parentOf(el);
    const root = el.getRootNode();
    const inShadow = root instanceof ShadowRoot;
    const ancestry = [];
    for (let p = parent; p; p = parentOf(p)) ancestry.push(label(p));
    const record = {
      schema: SCHEMA,
      identity: {
        node_id: null,
        tag: el.tagName.toLowerCase(),
        id: el.id || null,
        classes: Array.from(el.classList),
        selector: structural(el),
        oracle_key: getSelector(el),
        ancestry,
        child_index: childIndex(el),
      },
      tree: {
        in_dom: true,
        connected: el.isConnected,
        in_shadow_tree: inShadow,
        shadow_host: inShadow ? structural(root.host) : null,
        assigned_slot: el.assignedSlot ? structural(el.assignedSlot) : null,
        child_count: childCount(el),
        text_length: (el.textContent || '').length - harnessText(el),
      },
      style: {},
      box: {
        has_box: false, no_box_reason: null, border_box: null, padding_box: null, content_box: null,
        containing_block: null, formatting_context: null, grid_lines: null,
      },
      paint: {
        display_commands: null, command_kinds: null, clipped: null, clipped_by: null,
        outside_viewport: null, effective_opacity: null, in_opacity_layer: null, in_transform_layer: null,
      },
      notes,
      unavailable,
    };
    unavailable['identity.node_id'] = 'Chromium exposes no node id to script';
    for (const p of props) {
      // background-image is scrubbed in Node (record_scrub.mjs) before writing.
      const v = c.getPropertyValue(p);
      record.style[p] = v === '' ? null : v;
      if (v === '') unavailable[`style.${p}`] = 'empty in getComputedStyle';
    }
    const nc = 'not observable from script';
    unavailable['paint.display_commands'] = nc;
    unavailable['paint.command_kinds'] = nc;

    if (!hasBox(el)) {
      let reason;
      if (c.display === 'none') reason = 'display:none';
      else if (c.display === 'contents') reason = 'display:contents';
      else {
        let p = parent;
        while (p && !hasBox(p)) p = parentOf(p);
        reason = p && p !== parent ? `an ancestor has no box (nearest boxed ancestor: ${structural(p)})` : 'no client rects';
      }
      record.box.no_box_reason = reason;
      unavailable.box = 'no layout box';
      unavailable.paint = 'no layout box';
      out.push(record);
      continue;
    }

    const { border, padding, content } = boxes(el, c);
    record.box.has_box = true;
    record.box.border_box = r4(border.x, border.y, border.width, border.height);
    // Padding and content boxes are the bounding rect less computed border
    // and padding, which holds only while no transform on the chain scales,
    // rotates or skews; otherwise they are unknown (as on the engine side
    // for rotation; the engine maps a scale, which compares as unknown here).
    let axisTransformOnly = true;
    for (let p = el; p; p = parentOf(p)) {
      const t = cs(p).transform;
      if (t && t !== 'none') {
        const m = new DOMMatrixReadOnly(t);
        if (!m.is2D || m.a !== 1 || m.b !== 0 || m.c !== 0 || m.d !== 1) { axisTransformOnly = false; break; }
      }
    }
    if (axisTransformOnly) {
      record.box.padding_box = r4(...padding);
      record.box.content_box = r4(...content);
    } else {
      unavailable['box.padding_box'] = 'not derivable from the bounding rect under a scale, rotation or skew';
      unavailable['box.content_box'] = 'not derivable from the bounding rect under a scale, rotation or skew';
    }

    // Containing block, as far as script can know.
    let cb = null;
    if (c.position === 'fixed') cb = null;
    else if (c.position === 'absolute') {
      for (let p = parent; p; p = parentOf(p)) if (establishesAbsCb(cs(p))) { cb = p; break; }
    } else if (el !== document.documentElement) {
      for (let p = parent; p; p = parentOf(p)) if (BLOCK_CONTAINERS.has(cs(p).display)) { cb = p; break; }
    }
    record.box.containing_block = cb
      ? { selector: structural(cb), content_box: r4(...boxes(cb, cs(cb)).content) }
      : { selector: null, content_box: r4(0, 0, vw, vh) };

    const pd = parent ? cs(parent).display : null;
    let fc;
    if (c.position === 'fixed') fc = 'fixed';
    else if (c.position === 'absolute') fc = `absolute relative to ${cb ? structural(cb) : 'viewport'}`;
    else if (c.float !== 'none') fc = 'float';
    else if (c.display === 'table-cell') fc = 'table cell';
    else if (pd && isFlex(pd)) fc = `flex item of ${structural(parent)}`;
    else if (pd && isGrid(pd)) {
      fc = `grid item of ${structural(parent)}`;
      unavailable['box.grid_lines'] = 'Chromium does not expose resolved grid placement to script';
    } else if (c.display.startsWith('inline')) fc = 'inline';
    else fc = 'block';
    record.box.formatting_context = fc;

    // Paint facts: clip ancestors (an abspos box escapes clips below its
    // containing block), opacity and transform chains.
    let clippedBy = null;
    const abs = c.position === 'absolute';
    const fixed = c.position === 'fixed';
    for (let p = parent; p && !fixed; p = parentOf(p)) {
      const pc = cs(p);
      if (p === document.documentElement || p === document.body) break;
      // A clip applies to an abspos box only at or above its containing block.
      if (abs && !(cb && (p === cb || p.contains(cb)))) continue;
      if (pc.overflowX !== 'visible' || pc.overflowY !== 'visible') {
        const [x, y, w, h] = boxes(p, pc).padding;
        const e = 0.01;
        if (border.x < x - e || border.y < y - e || border.right > x + w + e || border.bottom > y + h + e) {
          clippedBy = p;
          break;
        }
      }
    }
    let opacity = 1, opacityLayer = false, transformLayer = false;
    for (let p = el; p; p = parentOf(p)) {
      const pc = cs(p);
      const o = parseFloat(pc.opacity);
      opacity *= o;
      if (o < 1) opacityLayer = true;
      if (pc.transform !== 'none') transformLayer = true;
    }
    Object.assign(record.paint, {
      clipped: !!clippedBy,
      clipped_by: clippedBy ? structural(clippedBy) : null,
      outside_viewport: border.right <= 0 || border.bottom <= 0 || border.x >= vw || border.y >= vh,
      effective_opacity: Math.round(opacity * 1000) / 1000,
      in_opacity_layer: opacityLayer,
      in_transform_layer: transformLayer,
    });
    out.push(record);
  }
  return { elements: out, total: all.length, scroll: [scrollX, scrollY], href: location.href };
}

function parseArgs(argv) {
  const opts = { width: 1280, height: 800, scale: 1, settle: 1500, maxElements: 5000, keepQuery: false };
  const pos = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const num = () => Number(argv[++i]);
    if (a === '--width') opts.width = num();
    else if (a === '--height') opts.height = num();
    else if (a === '--scale') opts.scale = num();
    else if (a === '--settle') opts.settle = num();
    else if (a === '--max-elements') opts.maxElements = num();
    else if (a === '--keep-query') opts.keepQuery = true;
    else pos.push(a);
  }
  [opts.target, opts.out] = pos;
  return opts;
}


async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (!opts.target || !opts.out) {
    console.error('usage: chrome_records.mjs <url|html-file> <out.json> [--width W --height H --scale S --settle MS --max-elements N --keep-query]');
    process.exit(2);
  }
  const isUrl = isUrlTarget(opts.target);
  const named = reportTarget(opts.target, isUrl, opts.keepQuery, process.cwd());
  const fnSrc = extractGetSelectorSource(
    readFileSync(join(__dirname, '..', 'parity_oracle', 'capture_baseline.mjs'), 'utf8'),
  );
  const result = { target: named, status: 'ok', error: null };
  const browser = await chromium.launch(getDeterministicLaunchOptions());
  let url;
  // An error message quotes the target in full: name it as the record does.
  const redact = (e) => redactTarget(String(e.message || e).split('\n')[0], [opts.target, url], named);
  try {
    let context;
    if (isUrl) {
      url = opts.target;
      context = await browser.newContext({
        viewport: { width: opts.width, height: opts.height },
        deviceScaleFactor: opts.scale,
        colorScheme: 'light',
        locale: 'en-US',
        extraHTTPHeaders: { 'Sec-CH-Prefers-Color-Scheme': 'light' },
      });
    } else {
      const abs = resolve(opts.target);
      if (!existsSync(abs)) throw new Error(`HTML file not found: ${opts.target}`);
      url = `file://${abs}`;
      context = await createDeterministicContext(browser, opts.width, opts.height, {
        applyParityReset: shouldApplyParityResetForHtmlPath(abs),
      });
    }
    const page = await context.newPage();
    let response = null;
    try {
      response = await page.goto(url, { waitUntil: isUrl ? 'load' : 'networkidle', timeout: NAV_TIMEOUT_MS });
    } catch (e) {
      result.nav_error = redact(e);
    }
    // A failed navigation loaded Chromium's error page, not the target, so
    // nothing is recorded. about: and data: URLs load with no response.
    if (result.nav_error || (!response && isUrl && !/^(about|data):/i.test(opts.target))) {
      throw new Error(result.nav_error || 'navigation got no response');
    }
    // An error page is recorded (it is what loaded) but the run is not ok.
    if (response && response.status() >= 400) {
      result.status = 'error';
      result.error = `HTTP ${response.status()}`;
    }
    await page.waitForTimeout(isUrl ? opts.settle : 50);
    const got = await page.evaluate(collect, [fnSrc, STYLE_PROPERTIES, opts.maxElements]);
    for (const r of got.elements) {
      const v = r.style['background-image'];
      if (v != null) r.style['background-image'] = backgroundImage(v, got.href);
    }
    const dump = {
      schema: 'hiwave-element-diagnostics/1',
      page: {
        // A local file is named by a relative path, never by host path.
        url: named,
        engine: { version: `chromium ${browser.version()}`, commit: null },
        viewport: [opts.width, opts.height],
        device_scale: isUrl ? opts.scale : 1,
        scroll: got.scroll,
        text_metrics_font_derived: true,
        unavailable: { 'engine.commit': 'not applicable to Chromium' },
      },
      total_elements: got.total,
      truncated: got.total > got.elements.length,
      elements: got.elements,
    };
    mkdirSync(dirname(resolve(opts.out)), { recursive: true });
    writeFileSync(opts.out, JSON.stringify(dump));
    result.records_path = opts.out;
    result.element_count = got.elements.length;
    result.truncated = dump.truncated;
    await context.close();
  } catch (e) {
    result.status = 'error';
    result.error = redact(e);
  } finally {
    await browser.close();
  }
  console.log(JSON.stringify(result));
  if (result.status !== 'ok') process.exit(1);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  await main();
}
