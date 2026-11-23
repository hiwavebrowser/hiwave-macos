// INLINE-MID-LINE follow-up (#1422, batch 124): two WPT cases moved away from Chrome when a span
// started mid-line. (1) css-text/letter-spacing/letter-spacing-200: a span that starts at a line
// end and holds a forced break (<br> or a preserved newline) lost a line. (2)
// css-position/position-absolute-in-inline-007b: a span that starts mid-line and is split by a
// block counted one line too many. Records each paragraph's / container's height and the span's
// fragments, in the tests' own markup, with Ahem from crates/rustkit-text/tests/fixtures. The same
// pages are the rustkit-engine test inline_mid_line_tests (the forced-break and block-split rows).
// Writes docs/diagnostics/2026-11-23/inline_mid_line_breaks_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/inline_mid_line_breaks_log.mjs
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const HERE = dirname(fileURLToPath(import.meta.url));
const RECEIPT = join(HERE, '../../docs/diagnostics/2026-11-23/inline_mid_line_breaks_cft148.json');
const AHEM = readFileSync(join(HERE, '../../crates/rustkit-text/tests/fixtures/Ahem.ttf')).toString('base64');
const FACE = `@font-face{font-family:Ahem;src:url(data:font/ttf;base64,${AHEM})}`;

// letter-spacing-200's box: 20px/1 Ahem, 5em wide, letter-spacing 1em on spans.
const LS = 'body{margin:0} .c{font:20px/1 Ahem;width:5em} span{letter-spacing:1em} p{margin:0} .pre{white-space:pre-wrap}';
// position-absolute-in-inline-007b's container, with and without its two columns.
const POS = 'body{margin:0} .c{font:20px/1 Ahem;inline-size:16em} .cols{block-size:3em;columns:2;column-fill:auto;column-gap:0} ' +
  '.rel{position:relative} .block{inline-size:8em;block-size:1em;background:black}';
const SPLIT = `XX
  <span id=s class="rel">
    XXXX<br>
    <div class="block"></div>
    XXXXX<br>
    XXXXXXXX<br>
    XXXXX<br>
    XXXXX
  </span>`;
// [label, style, body]; #p is the measured paragraph or container, #s the span.
const CASES = [
  ['span starting at a line end, holding <br>s', LS, '<div class=c><p id=p>1 2 3<span id=s><br>4 5<br></span>6 7 8</p></div>'],
  ['span starting at a line end, holding newlines (pre-wrap)', LS, '<div class=c><p id=p class=pre>1 2 3<span id=s>\n4 5\n</span>6 7 8</p></div>'],
  ['span between <br>s', LS, '<div class=c><p id=p>1 2 3<br><span id=s>4 5</span><br>6 7 8</p></div>'],
  ['the whole letter-spacing-200 internal-break box', LS,
    '<div class=c id=p><p><span>1 2</span></p><p>1 2 三<span>456</span>七 8 9</p><p>1 2 3<span id=s><br>4 5<br></span>6 7 8</p>' +
    '<p class="pre">1 2 3<span>\n4 5\n</span>6 7 8</p></div>'],
  ['span between newlines (pre-wrap)', LS, '<div class=c><p id=p class=pre>1 2 3\n<span id=s>4 5</span>\n6 7 8</p></div>'],
  ['the whole letter-spacing-200 external-break box', LS,
    '<div class=c id=p><p><span>1 2</span></p><p>1 2 三<span>456</span>七 8 9</p><p>1 2 3<br><span id=s>4 5</span><br>6 7 8</p>' +
    '<p class="pre">1 2 3\n<span>4 5</span>\n6 7 8</p></div>'],
  ['span after text, split by a block', POS, `<div class=c id=p>${SPLIT}</div>`],
  ['span after text, split by a block, two columns', POS, `<div class="c cols" id=p>${SPLIT}</div>`],
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
      const r = (e) => { if (!e) return null; const b = e.getBoundingClientRect(); return [b.x, b.y, b.width, b.height]; };
      const s = document.getElementById('s');
      return {
        font_loaded: document.fonts.check('20px Ahem'),
        height: document.getElementById('p').getBoundingClientRect().height,
        span: r(s),
        span_fragments: [...s.getClientRects()].map((b) => [b.x, b.y, b.width, b.height]),
      };
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
