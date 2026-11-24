// Row LOGICAL-SIZE: the flow-relative size properties (css-logical-1 §4.1): inline-size,
// block-size and their min-/max- forms. In horizontal-tb they are width / height / min-width /
// min-height / max-width / max-height. RustKit did not know them (WPT
// css-position/position-absolute-in-inline-007b sizes its boxes with them). Records each box's
// border-box size and getComputedStyle's reads of the logical and physical names. The same pages are
// the rustkit-engine test logical_size_tests.
// Writes docs/diagnostics/2026-11-24/logical_size_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/logical_size_log.mjs
import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const RECEIPT = join(dirname(fileURLToPath(import.meta.url)), '../../docs/diagnostics/2026-11-24/logical_size_cft148.json');
// [label, the #b box's style]; #b sits in a 400x300 #p, and holds one 50x50 block.
const CASES = [
  ['inline-size and block-size', 'inline-size:200px;block-size:50px'],
  ['percentages', 'inline-size:50%;block-size:20%'],
  ['em', 'font-size:10px;inline-size:12em;block-size:3em'],
  ['min-inline-size over width', 'width:100px;min-inline-size:300px'],
  ['max-inline-size on an auto width', 'max-inline-size:150px'],
  ['min-block-size over the content', 'min-block-size:80px'],
  ['max-block-size under height', 'height:100px;max-block-size:30px'],
  ['max-inline-size: none after a limit', 'max-width:120px;max-inline-size:none'],
  ['later inline-size wins over width', 'width:100px;inline-size:200px'],
  ['later width wins over inline-size', 'inline-size:200px;width:100px'],
  ['block-size auto keeps the content height', 'height:90px;block-size:auto'],
  ['inline-size: inherit', '--x:1', 'inherit'],
  ['box-sizing border-box', 'box-sizing:border-box;padding:10px;border:5px solid;inline-size:200px;block-size:60px'],
  ['inline-size: fit-content', 'inline-size:fit-content'],
];
// Prometheus's R1 nits on #1442: inherit into a box that does not fill its parent (a float), a
// stylesheet declaration under an inline one, !important, and CSS.supports.
// [label, #p's style, a stylesheet for #b, #b's inline style]
const SHEET_CASES = [
  ['inline-size: inherit on a float', 'width:300px;height:300px', '', 'float:left;inline-size:inherit'],
  ['block-size: inherit', 'width:400px;height:300px', '', 'block-size:inherit'],
  ['a sheet inline-size under an inline width', 'width:400px;height:300px', '#b{inline-size:200px}', 'width:100px'],
  ['a sheet width under an inline inline-size', 'width:400px;height:300px', '#b{width:100px}', 'inline-size:200px'],
  ['a sheet inline-size !important over an inline width', 'width:400px;height:300px', '#b{inline-size:200px !important}', 'width:100px'],
  ['a sheet width !important over an inline inline-size', 'width:400px;height:300px', '#b{width:100px !important}', 'inline-size:200px'],
];

const browser = await chromium.launch({
  ...getDeterministicLaunchOptions(),
  headless: true,
  executablePath: getPinnedExecutablePath({ required: true }),
});
const cases = [];
try {
  if (browser.version() !== PIN) throw new Error(`expected ${PIN}, got ${browser.version()}`);
  const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
  for (const [label, style, inherit] of CASES) {
    // `inherit` case: #p carries inline-size:250px and #b says inline-size:inherit.
    const outer = inherit ? 'width:400px;height:300px;inline-size:250px' : 'width:400px;height:300px';
    const own = inherit ? 'inline-size:inherit' : style;
    await page.setContent(`<!doctype html><style>body{margin:0}</style><div id=p style="${outer}"><div id=b style="${own}"><div style="width:50px;height:50px"></div></div></div>`);
    const got = await page.evaluate(() => {
      const b = document.getElementById('b');
      const r = b.getBoundingClientRect();
      const cs = getComputedStyle(b);
      const read = ['width', 'height', 'inline-size', 'block-size', 'min-width', 'min-inline-size', 'max-width',
        'max-inline-size', 'min-height', 'min-block-size', 'max-height', 'max-block-size'];
      return { width: r.width, height: r.height, computed: Object.fromEntries(read.map((p) => [p, cs.getPropertyValue(p)])) };
    });
    console.log(label, JSON.stringify(got));
    cases.push({ label, style: own, outer, ...got });
  }
  for (const [label, outer, sheet, own] of SHEET_CASES) {
    await page.setContent(`<!doctype html><style>body{margin:0} ${sheet}</style><div id=p style="${outer}"><div id=b style="${own}"><div style="width:50px;height:50px"></div></div></div>`);
    const got = await page.evaluate(() => {
      const b = document.getElementById('b');
      const r = b.getBoundingClientRect();
      const cs = getComputedStyle(b);
      return { width: r.width, height: r.height, computed: { 'inline-size': cs.getPropertyValue('inline-size'), 'block-size': cs.getPropertyValue('block-size') } };
    });
    console.log(label, JSON.stringify(got));
    cases.push({ label, style: own, outer, sheet, ...got });
  }
  const supports = await page.evaluate(() => Object.fromEntries([
    ['inline-size', '10px'], ['block-size', '10px'], ['min-inline-size', '10px'], ['max-inline-size', 'none'],
    ['min-block-size', '10px'], ['max-block-size', 'none'], ['inline-size', 'red'],
  ].map(([p, v]) => [`${p}: ${v}`, CSS.supports(p, v)])));
  console.log('CSS.supports', JSON.stringify(supports));
  cases.push({ label: 'CSS.supports', supports });
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, platform: process.platform, cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
