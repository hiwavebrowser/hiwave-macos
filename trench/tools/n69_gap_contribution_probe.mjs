/**
 * Chrome-vs-Chrome probe: what does a GAP contribute to a flex container's
 * max-content width, and how does that decompose over the container's items?
 *
 * Night 69's question. `grid::own_max_content_width` reads a flex container's
 * main-axis gap as `match style.column_gap { Length::Px(g) => g, _ => 0.0 }`,
 * so a `rem` gap contributes ZERO to the container's max-content contribution
 * while layout resolves it properly. Night 68 measured the naive fix at
 * 9 axes better / 11 WORSE and deliberately did not land it. This probe exists
 * to decide whether those regressions are the fix over-correcting or a
 * pre-existing item error that the missing gap was CANCELLING.
 *
 * For every row flex container with a resolved main-axis gap it prints
 * Chrome's ground truth:
 *
 *   container max-content  vs  SUM(item max-content) + (n-1)*gap
 *
 * Those two are the same quantity in css-flexbox-1 §9.9, so the row is a
 * self-check on the probe; the useful columns are the per-item max-contents,
 * which are what RustKit's intrinsics must be compared against ONE AT A TIME.
 * A container whose total error equals the sum of its items' errors has no
 * container-level defect left.
 *
 * Measured by setting `width: max-content` on the element and reading its
 * border box, then restoring — the element's own intrinsic size, not its used
 * size, which is what a contribution is.
 *
 * No RustKit is involved and no baseline is read, so this is not a receipt. The
 * ADVANCES printed are this seat's font stack; the ARITHMETIC (does the gap
 * term close?) is platform-independent.
 *
 * Diagnostic only, no mutation-checked guards — do not wire into CI.
 *
 *   PARITY_CHROME_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome \
 *     node trench/tools/n69_gap_contribution_probe.mjs <page.html> [width] [height]
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
  console.error('usage: n69_gap_contribution_probe.mjs <page.html> [width] [height]');
  process.exit(2);
}

const browser = await chromium.launch(getDeterministicLaunchOptions());
const context = await createDeterministicContext(browser, width, height);
const page = await context.newPage();
await page.goto('file://' + resolve(fixture));

const rows = await page.evaluate(() => {
  // Same selector rule the baseline capture uses, so a row here names the same
  // element a layout-rects.json row does.
  const sel = (el) => {
    if (el.id) return `#${el.id}`;
    const t = el.tagName.toLowerCase();
    const c = el.className && typeof el.className === 'string' ? el.className.trim() : '';
    return c ? `${t}.${c}` : t;
  };
  const intrinsic = (el, kw) => {
    const prev = el.style.width;
    el.style.width = kw;
    const w = el.getBoundingClientRect().width;
    el.style.width = prev;
    return w;
  };
  const out = [];
  for (const el of document.querySelectorAll('*')) {
    const cs = getComputedStyle(el);
    if (cs.display !== 'flex' && cs.display !== 'inline-flex') continue;
    const dir = cs.flexDirection || 'row';
    if (!dir.startsWith('row')) continue;
    const gap = parseFloat(cs.columnGap);
    if (!Number.isFinite(gap) || gap <= 0) continue;
    // css-flexbox-1 §4: every in-flow child box is a flex item, AND each
    // contiguous run of text directly inside the container is wrapped in an
    // ANONYMOUS flex item. A white-space-only run is not rendered and takes no
    // gap slot; a run like the `/` and `+` in new_tab's `.shortcut` IS one, so
    // an element-children-only count reports 4 items where the container has 6
    // and understates the gap term by two slots. RustKit's own walk counts its
    // Text children the same way, so a probe that skipped them would compare
    // two different item counts and call the difference an engine defect.
    const nodes = [...el.childNodes].filter((c) => {
      if (c.nodeType === 1) return getComputedStyle(c).display !== 'none';
      if (c.nodeType === 3) return c.textContent.trim() !== '';
      return false;
    });
    if (nodes.length < 2) continue;
    // The container's intrinsic width is a BORDER box; the items sum to a
    // CONTENT width. Comparing the two directly reports the container's own
    // padding and border as a residual.
    const pb =
      parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight) +
      parseFloat(cs.borderLeftWidth) + parseFloat(cs.borderRightWidth);
    const measure = (c) => {
      if (c.nodeType === 3) {
        // A text run has no box to set `width` on. Its max-content width is the
        // run's own advance, which is what a Range reports when it does not wrap.
        const r = document.createRange();
        r.selectNodeContents(c);
        const w = r.getBoundingClientRect().width;
        return { selector: `#text \"${c.textContent.trim()}\"`, max: w, min: w, margins: 0, isText: true };
      }
      const ccs = getComputedStyle(c);
      const m = parseFloat(ccs.marginLeft) + parseFloat(ccs.marginRight);
      return { selector: sel(c), max: intrinsic(c, 'max-content'), min: intrinsic(c, 'min-content'), margins: m, isText: false };
    };
    out.push({
      selector: sel(el),
      gap,
      wrap: cs.flexWrap,
      n: nodes.length,
      pb,
      containerMax: intrinsic(el, 'max-content'),
      containerMin: intrinsic(el, 'min-content'),
      items: nodes.map(measure),
    });
  }
  return out;
});

await browser.close();

console.log(`# ${fixture}  ${width}x${height}   row flex containers with a main-axis gap: ${rows.length}`);
for (const r of rows) {
  const sumMax = r.items.reduce((a, i) => a + i.max + i.margins, 0);
  const gaps = r.gap * (r.n - 1);
  const predicted = sumMax + gaps + r.pb;
  const resid = r.containerMax - predicted;
  console.log(
    `\n${r.selector}   gap=${r.gap}  n=${r.n}  wrap=${r.wrap}\n` +
    `  container max-content      ${r.containerMax.toFixed(3)}\n` +
    `  SUM(item max)+(n-1)*gap+pb ${predicted.toFixed(3)}   ` +
    `[items ${sumMax.toFixed(3)} + gaps ${gaps.toFixed(3)} + pb ${r.pb.toFixed(3)}]\n` +
    `  residual                   ${resid.toFixed(3)}`
  );
  for (const i of r.items) {
    console.log(`    item ${i.selector.padEnd(34)} max ${i.max.toFixed(3).padStart(9)}  min ${i.min.toFixed(3).padStart(9)}  margins ${i.margins.toFixed(3)}${i.isText ? '   (anonymous text run)' : ''}`);
  }
}
