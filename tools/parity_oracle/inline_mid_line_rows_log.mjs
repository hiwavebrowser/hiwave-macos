// INLINE-MID-LINE follow-up (#1422, Prometheus HOLD at 61853644): which characters Chrome puts on
// each line of css-text/letter-spacing/letter-spacing-200's box 3, row by row. A height can come
// out right with a line's content on the wrong line; this records the lines themselves. Each
// paragraph of the box is walked one character at a time (a Range per character), characters are
// grouped by the top of their rect, whitespace is dropped, and each line is the remaining
// characters in order. Run with Ahem (as written) and with Arial (a face that breaks other rows).
// The same rows are the rustkit-engine test inline_mid_line_tests (letter_spacing_200_box_3_*).
// Writes docs/diagnostics/2026-11-26/inline_mid_line_rows_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/inline_mid_line_rows_log.mjs
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const HERE = dirname(fileURLToPath(import.meta.url));
const RECEIPT = join(HERE, '../../docs/diagnostics/2026-11-26/inline_mid_line_rows_cft148.json');
const AHEM = readFileSync(join(HERE, '../../crates/rustkit-text/tests/fixtures/Ahem.ttf')).toString('base64');
const FACE = `@font-face{font-family:Ahem;src:url(data:font/ttf;base64,${AHEM})}`;

// letter-spacing-200's stylesheet, with the box's face as a parameter.
const STYLE = (face) => `body{margin:0} .contain{font:20px/1 ${face};width:5em;border:solid blue;margin:1em;float:left} ` +
  'span,.test{letter-spacing:1em} .pre,.control p{white-space:pre-wrap} p{margin:0}';
const NL = String.fromCharCode(10);
// Box 3 as written (external breaks).
const BOX3 = '<div class="contain" id=p><p><span>1 2</span></p><p>1 2 三<span>456</span>七 8 9</p>' +
  '<p>1 2 3<br><span>4 5</span><br>6 7 8</p><p class="pre">1 2 3' + NL + '<span>4 5</span>' + NL + '6 7 8</p></div>';
const CASES = [
  ['box 3, Ahem', STYLE('Ahem'), BOX3],
  ['box 3, Arial', STYLE('Arial'), BOX3],
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
  for (const [label, style, body] of CASES) {
    await page.setContent(`<!doctype html><style>${FACE} ${style}</style>${body}`);
    await page.evaluate(() => document.fonts.ready);
    const got = await page.evaluate(() => {
      const box = document.getElementById('p');
      const rows = [...box.children].map((p) => {
        const tops = new Map();
        const walker = document.createTreeWalker(p, NodeFilter.SHOW_TEXT);
        for (let n = walker.nextNode(); n; n = walker.nextNode()) {
          for (let i = 0; i < n.data.length; i++) {
            if (/\s/.test(n.data[i])) continue;
            const r = document.createRange();
            r.setStart(n, i);
            r.setEnd(n, i + 1);
            const rect = r.getClientRects()[0];
            const top = Math.round(rect.top);
            tops.set(top, (tops.get(top) || '') + n.data[i]);
          }
        }
        return [...tops.entries()].sort((a, b) => a[0] - b[0]).map((e) => e[1]);
      });
      return { height: box.getBoundingClientRect().height, rows };
    });
    console.log(label, JSON.stringify(got));
    cases.push({ label, style, body, ...got });
  }
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, platform: process.platform, cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
