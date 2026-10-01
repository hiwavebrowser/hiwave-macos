/**
 * Chrome-vs-Chrome probe: what IS a form control's min-content width?
 *
 * Answers one question the corpus cannot: for each control type, does Chrome's
 * intrinsic MIN differ from its intrinsic MAX, and if so by what rule. Night 68
 * used it to establish that a button floors at its WIDEST WORD plus the
 * horizontal padding/border (and that every other control has min == max), which
 * is why `grid::form_control_min_content_width` is a text rule rather than a
 * delegation to `form_control_intrinsic_size`.
 *
 * No RustKit is involved and no baseline is read, so this is not a receipt and
 * carries no seat confound in the usual sense — it is Chrome measured against
 * itself, and the RULE it establishes is platform-independent even though the
 * ADVANCES it prints are this seat's font stack.
 *
 * Diagnostic only, no mutation-checked guards — do not wire into CI.
 *
 *   PARITY_CHROME_PATH=/opt/pw-browsers/chromium-1194/chrome-linux/chrome \
 *     node trench/tools/n68_control_intrinsic_probe.mjs <probe.html>
 *
 * The fixture wraps each control in `width: min-content` and `width:
 * max-content` siblings and reads every [id]'s border box. A flex line narrower
 * than the sum of the floors, and a float in a parent narrower than one floor,
 * are the two consumer-level cases — they show Chrome OVERFLOWING rather than
 * shrinking a control below its text.
 */
import { dirname, join, resolve } from 'path';
import { fileURLToPath } from 'url';

// `playwright` and the shared launch options both live under
// tools/parity_oracle, which is where the dependency is installed — resolved by
// path rather than by bare specifier so this runs from the repo root.
const HERE = dirname(fileURLToPath(import.meta.url));
const ORACLE = resolve(HERE, '../../tools/parity_oracle');
const { chromium } = await import(join(ORACLE, 'node_modules/playwright/index.mjs'));
const { getDeterministicLaunchOptions, createDeterministicContext } =
  await import(join(ORACLE, 'deterministic.mjs'));

const fixture = process.argv[2];
if (!fixture) {
  console.error('usage: n68_control_intrinsic_probe.mjs <probe.html>');
  process.exit(2);
}

const browser = await chromium.launch(getDeterministicLaunchOptions());
const context = await createDeterministicContext(browser, 1280, 900);
const page = await context.newPage();
await page.goto('file://' + resolve(fixture));
const rects = await page.evaluate(() => {
  const out = {};
  document.querySelectorAll('[id]').forEach((el) => {
    const bb = el.getBoundingClientRect();
    out[el.id] = [+bb.width.toFixed(3), +bb.height.toFixed(3)];
  });
  return out;
});
console.log('id'.padEnd(6), 'width'.padStart(9), 'height'.padStart(8));
for (const [id, [w, h]] of Object.entries(rects)) {
  console.log(id.padEnd(6), String(w).padStart(9), String(h).padStart(8));
}
await browser.close();
