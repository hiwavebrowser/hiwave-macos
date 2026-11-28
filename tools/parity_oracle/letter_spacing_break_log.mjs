// LETTER-SPACING-BREAK: where Chrome breaks lines of text that carries letter-spacing or
// word-spacing. RustKit fits lines with the shaper's widths and adds the spacing afterwards, so a
// spaced line it thinks fits can be wider than its box (WPT css-text/letter-spacing/
// letter-spacing-200: `1 2` at letter-spacing 1em in a 5em box stays on one line, box 3 is 206px
// tall where Chrome's is 266). Each case's `#p` is walked one character at a time (a Range per
// character), characters are grouped by the top of their rect, whitespace is dropped, and each
// line is the remaining characters in order. The same cases are the rustkit-engine test
// letter_spacing_break_tests.
// Writes docs/diagnostics/2026-11-26/letter_spacing_break_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/letter_spacing_break_log.mjs
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const HERE = dirname(fileURLToPath(import.meta.url));
const RECEIPT = join(HERE, '../../docs/diagnostics/2026-11-26/letter_spacing_break_cft148.json');
const AHEM = readFileSync(join(HERE, '../../crates/rustkit-text/tests/fixtures/Ahem.ttf')).toString('base64');
const FACE = `@font-face{font-family:Ahem;src:url(data:font/ttf;base64,${AHEM})}`;

const NL = String.fromCharCode(10);
const SENTENCE = 'The quick brown fox jumps over the lazy dog and keeps on running far away';
const AH = 'body{margin:0} #p{font:20px/1 Ahem;width:100px}';
const AR = 'body{margin:0} #p{font:16px/1.25 Arial;width:200px}';
// letter-spacing-200's stylesheet and its boxes 2 and 3 as written.
const WPT = 'body{margin:0} .contain{font:20px/1 Ahem;width:5em;border:solid blue;margin:1em;float:left} ' +
  'span,.test{letter-spacing:1em} .pre,.control p{white-space:pre-wrap} p{margin:0}';
const HEAD = '<p><span>1 2</span></p><p>1 2 三<span>456</span>七 8 9</p>';
const BOX2 = HEAD + '<p>1 2 3<span><br>4 5<br></span>6 7 8</p><p class="pre">1 2 3<span>' + NL + '4 5' + NL + '</span>6 7 8</p>';
const BOX3 = HEAD + '<p>1 2 3<br><span>4 5</span><br>6 7 8</p><p class="pre">1 2 3' + NL + '<span>4 5</span>' + NL + '6 7 8</p>';

// [label, style, body]; `#p` is the measured block.
const CASES = [
  ['Ahem, letter-spacing 1em on the block, "1 2"', AH + ' #p{letter-spacing:1em}', '<div id=p>1 2</div>'],
  ['Ahem, letter-spacing 1em on the block, "1 2 3 4"', AH + ' #p{letter-spacing:1em}', '<div id=p>1 2 3 4</div>'],
  ['Ahem, letter-spacing 1em on a span, "1 2"', AH + ' span{letter-spacing:1em}', '<div id=p><span>1 2</span></div>'],
  ['Ahem, word-spacing 40px on the block, "1 2 3"', AH + ' #p{word-spacing:40px}', '<div id=p>1 2 3</div>'],
  ['Arial, letter-spacing 2px', AR + ' #p{letter-spacing:2px}', `<div id=p>${SENTENCE}</div>`],
  ['Arial, letter-spacing 0.2em', AR + ' #p{letter-spacing:0.2em}', `<div id=p>${SENTENCE}</div>`],
  ['Arial, word-spacing 8px', AR + ' #p{word-spacing:8px}', `<div id=p>${SENTENCE}</div>`],
  ['Arial, letter-spacing -1px', AR + ' #p{letter-spacing:-1px}', `<div id=p>${SENTENCE}</div>`],
  ['Arial, no spacing (control)', AR, `<div id=p>${SENTENCE}</div>`],
  ['letter-spacing-200 box 2 as written', WPT, `<div class="contain" id=p>${BOX2}</div>`],
  ['letter-spacing-200 box 3 as written', WPT, `<div class="contain" id=p>${BOX3}</div>`],
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
      const tops = new Map();
      const walker = document.createTreeWalker(box, NodeFilter.SHOW_TEXT);
      for (let n = walker.nextNode(); n; n = walker.nextNode()) {
        for (let i = 0; i < n.data.length; i++) {
          if (/\s/.test(n.data[i])) continue;
          const r = document.createRange();
          r.setStart(n, i);
          r.setEnd(n, i + 1);
          const top = Math.round(r.getClientRects()[0].top);
          tops.set(top, (tops.get(top) || '') + n.data[i]);
        }
      }
      return {
        font_loaded: document.fonts.check('20px Ahem'),
        height: box.getBoundingClientRect().height,
        lines: [...tops.entries()].sort((a, b) => a[0] - b[0]).map((e) => e[1]),
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
