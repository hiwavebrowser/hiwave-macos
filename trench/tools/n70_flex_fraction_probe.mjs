/**
 * Chrome-vs-Chrome probe: how does a ROW flex container's max-content main size
 * decompose over its items — by the RAW SUM rule RustKit uses, or by the
 * max-content flex fraction of css-flexbox-1 §9.9.1?
 *
 * Night 70's question. 09-29 recorded "own_max_content_width does not model
 * flex factors" as a 300-400px claim on `settings`, citing
 * `n69_gap_contribution_probe.mjs`: `div.blocklist-add` Chrome 266.188 against
 * an item sum of 652. That probe measures an item's intrinsic size by setting
 * `width: max-content` ON THE ITEM, which is a no-op for a flex item whose
 * flex-basis is not `auto` — `flex: 1` means basis `0%`, so the width property
 * never reaches the base size and the number read back is the item's USED width
 * inside its full-width container. Hence "652": 593.813 + 58.188 is exactly the
 * container's 660px used width, not a sum of contributions.
 *
 * This probe measures the two quantities §9.9.1 actually needs, each by
 * neutralising only the factor it is about and leaving the item's own `width`
 * declaration alone:
 *
 *   contribution  `flex: 0 0 auto`  (grow and shrink off, basis <- width/content)
 *   flex base     `flex-grow: 0; flex-shrink: 0`  (the item's own basis kept)
 *
 * and prints, per container, Chrome's own max-content beside both predictions:
 *
 *   RAW      SUM(outer contribution) + (n-1)*gap + pb        <- RustKit's rule
 *   §9.9.1   per-item base + factor * chosen fraction, + gaps + pb
 *
 * A container where the two predictions agree cannot tell the rules apart. The
 * useful rows are the ones where they disagree, and which of the two Chrome
 * matches there is the whole finding.
 *
 * No RustKit is involved and no baseline is read, so this is not a receipt. The
 * ADVANCES are this seat's font stack; whether a decomposition CLOSES is
 * platform-independent.
 *
 * Diagnostic only, no mutation-checked guards — do not wire into CI.
 *
 *   PARITY_CHROME_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome \
 *     node trench/tools/n70_flex_fraction_probe.mjs <page.html> [width] [height]
 */
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ORACLE = resolve(HERE, '../../tools/parity_oracle');
const { chromium } = await import(join(ORACLE, 'node_modules/playwright/index.mjs'));
const { getDeterministicLaunchOptions, createDeterministicContext } =
  await import(join(ORACLE, 'deterministic.mjs'));

const fixture = process.argv[2];
const width = Number(process.argv[3] || 1024);
const height = Number(process.argv[4] || 768);
if (!fixture) {
  console.error('usage: n70_flex_fraction_probe.mjs <page.html> [width] [height]');
  process.exit(2);
}

const browser = await chromium.launch(getDeterministicLaunchOptions());
const context = await createDeterministicContext(browser, width, height);
const page = await context.newPage();
await page.goto('file://' + resolve(fixture));

const rows = await page.evaluate(() => {
  const sel = (el) => {
    if (el.id) return `#${el.id}`;
    const t = el.tagName.toLowerCase();
    const c = el.className && typeof el.className === 'string' ? el.className.trim() : '';
    return c ? `${t}.${c}` : t;
  };
  // Read a width back with `decls` temporarily forced onto the element's inline
  // style, then restore every property touched. The item's own `width` is NEVER
  // in `decls`: a contribution is sized BY the width declaration, so overriding
  // it measures a different box.
  const withStyle = (el, decls) => {
    const saved = Object.keys(decls).map((k) => [k, el.style.getPropertyValue(k), el.style.getPropertyPriority(k)]);
    for (const [k, v] of Object.entries(decls)) el.style.setProperty(k, v, 'important');
    const w = el.getBoundingClientRect().width;
    for (const [k, v, p] of saved) {
      if (v) el.style.setProperty(k, v, p); else el.style.removeProperty(k);
    }
    return w;
  };
  // The COMPUTED width keyword, read where it cannot resolve to a used value.
  const authorWidth = (el) => {
    const prevDisplay = el.style.getPropertyValue('display');
    const prevPrio = el.style.getPropertyPriority('display');
    el.style.setProperty('display', 'none', 'important');
    const w = getComputedStyle(el).width;
    if (prevDisplay) el.style.setProperty('display', prevDisplay, prevPrio);
    else el.style.removeProperty('display');
    return w;
  };
  const out = [];
  for (const el of document.querySelectorAll('*')) {
    const cs = getComputedStyle(el);
    if (cs.display !== 'flex' && cs.display !== 'inline-flex') continue;
    if (!(cs.flexDirection || 'row').startsWith('row')) continue;
    // Anonymous text-run items (css-flexbox-1 §4) have no box to neutralise
    // factors on. A container holding one is reported with `textItems` set and
    // its predictions marked incomparable rather than silently measured wrong.
    let textItems = 0;
    const nodes = [];
    for (const c of el.childNodes) {
      if (c.nodeType === 1) {
        const ccs = getComputedStyle(c);
        if (ccs.display === 'none') continue;
        // css-flexbox-1 §4: an absolutely-positioned child is NOT a flex item.
        // `settings`' `.toggle-slider { position: absolute }` is the corpus's
        // instance, and counting it made the container look 48px wider than
        // Chrome and 2px wider on the min side — a probe defect that reads as
        // evidence for whichever rule happens to be nearer.
        if (ccs.position === 'absolute' || ccs.position === 'fixed') continue;
        nodes.push(c);
      }
      else if (c.nodeType === 3 && c.textContent.trim() !== '') { textItems++; }
    }
    if (nodes.length + textItems < 2) continue;
    // A box with no client rect is not laid out (an ancestor is `display: none`
    // — `settings`' vault-unlock row is the instance). Every width read from it
    // is 0, which scores as a match against whichever prediction is also 0.
    if (el.getClientRects().length === 0) continue;
    const gap = parseFloat(cs.columnGap) || 0;
    const pb = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight) +
               parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth);
    const items = nodes.map((c) => {
      const ccs = getComputedStyle(c);
      return {
        selector: sel(c),
        margins: parseFloat(ccs.marginLeft) + parseFloat(ccs.marginRight),
        grow: parseFloat(ccs.flexGrow),
        shrink: parseFloat(ccs.flexShrink),
        basis: ccs.flexBasis,
        // grow and shrink off: used size = flex base size, clamped by min/max.
        base: withStyle(c, { 'flex-grow': '0', 'flex-shrink': '0' }),
        // basis <- width (or content when width is auto), no flexing:
        // the item's max-content contribution.
        contribution: withStyle(c, { flex: '0 0 auto' }),
        // The MIN-content contribution. A definite `width` still determines it
        // (css-sizing-3 §5.2.1), so `width: min-content` is forced only where
        // the item's author width is `auto` and there is nothing to keep.
        //
        // `getComputedStyle(c).width` is NOT the test for that: on a rendered
        // element it returns the USED width in px and never the keyword, so an
        // `=== 'auto'` check is false for every item and this measurement
        // silently returns the max-content contribution instead. `authorWidth`
        // below reads the computed value through a `display: none` box, where
        // there is no used value to resolve to and the keyword survives.
        minContribution: authorWidth(c) === 'auto'
          ? withStyle(c, { flex: '0 0 auto', width: 'min-content' })
          : withStyle(c, { flex: '0 0 auto' }),
        authorWidth: authorWidth(c),
      };
    });
    out.push({
      selector: sel(el),
      gap, pb, wrap: cs.flexWrap, textItems,
      n: nodes.length + textItems,
      // `flex: 0 0 auto` on the CONTAINER for the same reason it is set on an
      // item: where the container is itself a flex item with a non-`auto`
      // basis (`chrome_rustkit`'s `div.url-bar`, `div.tab`, `settings`'
      // `div.setting-control`), the `width` forced here never reaches its used
      // size and the number read back is the used width — 906.000 for a
      // container whose min-content is 196.
      containerMax: withStyle(el, { flex: '0 0 auto', width: 'max-content' }),
      containerMin: withStyle(el, { flex: '0 0 auto', width: 'min-content' }),
      items,
    });
  }
  return out;
});

await browser.close();

// css-flexbox-1 §9.9.1, single line, on the items we can measure.
function nineNineOne(r) {
  let chosen = -Infinity;
  for (const i of r.items) {
    const outerBase = i.base + i.margins;
    const diff = i.contribution + i.margins - outerBase;
    const f = diff > 0
      ? diff / Math.max(i.grow, 1)
      : diff / (Math.max(i.shrink, 1) * Math.max(i.base, 1e-6));
    chosen = Math.max(chosen, f);
  }
  if (!Number.isFinite(chosen)) return null;
  let sum = 0;
  for (const i of r.items) {
    const factor = chosen < 0 ? Math.max(i.shrink, 1) * i.base : i.grow;
    sum += i.base + factor * chosen + i.margins;
  }
  return sum + r.gap * (r.n - 1) + r.pb;
}

const fmt = (v) => (v === null ? '     n/a' : v.toFixed(3).padStart(9));
console.log(`# ${fixture}  ${width}x${height}   row flex containers with >=2 items: ${rows.length}`);
let disagree = 0;
let minSumClose = 0, minMaxClose = 0, minNeither = 0, minComparable = 0;
for (const r of rows) {
  const raw = r.items.reduce((a, i) => a + i.contribution + i.margins, 0) + r.gap * (r.n - 1) + r.pb;
  const spec = r.textItems ? null : nineNineOne(r);
  const rawErr = r.containerMax - raw;
  const specErr = spec === null ? null : r.containerMax - spec;
  const tell = spec !== null && Math.abs(raw - spec) > 0.01;
  if (tell) disagree++;
  console.log(
    `\n${r.selector}   gap=${r.gap}  n=${r.n}${r.textItems ? ` (${r.textItems} anonymous text)` : ''}  wrap=${r.wrap}` +
    `${tell ? '   <== RULES DISAGREE' : ''}\n` +
    `  Chrome max-content  ${fmt(r.containerMax)}\n` +
    `  RAW sum  (RustKit)  ${fmt(raw)}   residual ${fmt(rawErr)}\n` +
    `  9.9.1 fraction      ${fmt(spec)}   residual ${fmt(specErr)}`
  );
  // MIN-content half. css-flexbox-1 §9.9.1: the min-content main size of a
  // single-line flex container is computed the same way as the max-content main
  // size with the items' MIN-content contributions, so a nowrap row container
  // SUMS them plus its gaps. `own_min_content_width` has no flex arm at all, so
  // its generic walk takes the LARGEST block-level child and drops every gap —
  // the two rules are printed side by side because that difference is the whole
  // question. A `wrap` container is not the sum rule (each item may take its own
  // line), so it is marked incomparable rather than scored against either.
  const minSum = r.items.reduce((a, i) => a + i.minContribution + i.margins, 0) + r.gap * (r.n - 1) + r.pb;
  const minMax = r.items.reduce((a, i) => Math.max(a, i.minContribution + i.margins), 0) + r.pb;
  const comparable = !r.textItems && r.wrap === 'nowrap';
  const sumErr = r.containerMin - minSum;
  const maxErr = r.containerMin - minMax;
  if (comparable) {
    minComparable++;
    const sumClose = Math.abs(sumErr) <= 0.01;
    const maxClose = Math.abs(maxErr) <= 0.01;
    if (sumClose) minSumClose++;
    if (maxClose) minMaxClose++;
    if (!sumClose && !maxClose) minNeither++;
  }
  console.log(
    `  Chrome min-content  ${fmt(r.containerMin)}` +
    `${comparable ? '' : '   (incomparable: ' + (r.textItems ? 'anonymous text' : 'wrap') + ')'}\n` +
    `  SUM+gaps  (9.9.1)   ${fmt(minSum)}   residual ${fmt(sumErr)}\n` +
    `  MAX child (RustKit) ${fmt(minMax)}   residual ${fmt(maxErr)}` +
    `${comparable && Math.abs(minSum - minMax) > 0.01 ? '   <== MIN RULES DISAGREE' : ''}`
  );
  for (const i of r.items) {
    console.log(`    ${i.selector.padEnd(30)} contrib ${i.contribution.toFixed(3).padStart(9)}  min ${i.minContribution.toFixed(3).padStart(9)}` +
      `  base ${i.base.toFixed(3).padStart(9)}  grow ${i.grow}  shrink ${i.shrink}  basis ${i.basis}  width ${i.authorWidth}  margins ${i.margins}`);
  }
}
console.log(`\n# containers where RAW and 9.9.1 disagree by >0.01px: ${disagree} of ${rows.length}`);
console.log(`# min-content, ${minComparable} comparable (nowrap, no anonymous text):` +
  ` SUM+gaps matches Chrome on ${minSumClose}, MAX-child on ${minMaxClose}, neither on ${minNeither}`);
