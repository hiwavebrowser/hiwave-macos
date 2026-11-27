// PRE-WRAP-NEWLINE-BEFORE-INLINE: a preserved newline at the end of a text run ends the line, so an
// inline box after it starts the next one. Chrome's lines, row by row, for paragraphs whose text
// ends with a preserved newline right before an inline (plain, nested, letter-spaced), under each
// white-space value that keeps newlines, with a white-space: normal control. Each paragraph is
// walked one character at a time (a Range per character), characters are grouped by the top of
// their rect, whitespace is dropped, and each line is the remaining characters in order; the
// paragraph's height is recorded too. Ahem 20px/1 (the WPT face) and Arial.
// The same rows are the rustkit-engine test pre_wrap_newline_before_inline_tests.
// Writes docs/diagnostics/2026-11-27/pre_wrap_newline_before_inline_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/pre_wrap_newline_before_inline_log.mjs
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const HERE = dirname(fileURLToPath(import.meta.url));
const RECEIPT = join(HERE, '../../docs/diagnostics/2026-11-27/pre_wrap_newline_before_inline_cft148.json');
const AHEM = readFileSync(join(HERE, '../../crates/rustkit-text/tests/fixtures/Ahem.ttf')).toString('base64');
const FACE = `@font-face{font-family:Ahem;src:url(data:font/ttf;base64,${AHEM})}`;
const NL = String.fromCharCode(10);

// (label, white-space, paragraph markup); each is laid out 300px and 100px wide.
const PARAGRAPHS = [
  ['newline before a span', 'pre-wrap', `1 2${NL}<span>4</span>6 7 8`],
  ['newline before a span, pre-line', 'pre-line', `1 2${NL}<span>4</span>6 7 8`],
  ['newline before a span, pre', 'pre', `1 2${NL}<span>4</span>6 7 8`],
  ['newline before a span, break-spaces', 'break-spaces', `1 2${NL}<span>4</span>6 7 8`],
  ['newline before and after a span', 'pre-wrap', `1 2 3${NL}<span>4 5</span>${NL}6 7 8`],
  ['newline before nested inlines', 'pre-wrap', `1 2${NL}<b><i>4</i></b>6 7 8`],
  ['two newlines before a span', 'pre-wrap', `1 2${NL}${NL}<span>4</span>6`],
  ['newline before a letter-spaced span', 'pre-wrap', `1 2${NL}<span style="letter-spacing:1em">4</span>6 7 8`],
  ['newline before a span, normal white-space', 'normal', `1 2${NL}<span>4</span> 6 7 8`],
];
const WIDTHS = [300, 100];
const FACES = ['Ahem', 'Arial'];

const browser = await chromium.launch({
  ...getDeterministicLaunchOptions(),
  headless: true,
  executablePath: getPinnedExecutablePath({ required: true }),
});
const cases = [];
try {
  if (browser.version() !== PIN) throw new Error(`expected ${PIN}, got ${browser.version()}`);
  const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
  for (const face of FACES) {
    for (const width of WIDTHS) {
      const style = `body{margin:0} p{margin:0;font:20px/1 ${face};width:${width}px}`;
      const body = PARAGRAPHS.map(([, ws, markup], i) => `<p id=p${i} style="white-space:${ws}">${markup}</p>`).join('');
      await page.setContent(`<!doctype html><style>${FACE} ${style}</style>${body}`);
      await page.evaluate(() => document.fonts.ready);
      const got = await page.evaluate((n) => {
        const out = [];
        for (let i = 0; i < n; i++) {
          const p = document.getElementById('p' + i);
          const tops = new Map();
          const walker = document.createTreeWalker(p, NodeFilter.SHOW_TEXT);
          for (let t = walker.nextNode(); t; t = walker.nextNode()) {
            for (let k = 0; k < t.data.length; k++) {
              if (/\s/.test(t.data[k])) continue;
              const r = document.createRange();
              r.setStart(t, k);
              r.setEnd(t, k + 1);
              const top = Math.round(r.getClientRects()[0].top);
              tops.set(top, (tops.get(top) || '') + t.data[k]);
            }
          }
          out.push({
            height: p.getBoundingClientRect().height,
            rows: [...tops.entries()].sort((a, b) => a[0] - b[0]).map((e) => e[1]),
          });
        }
        return out;
      }, PARAGRAPHS.length);
      got.forEach((g, i) => {
        const [label, ws, markup] = PARAGRAPHS[i];
        console.log(`${face} ${width}px ${label}: ${g.height} ${JSON.stringify(g.rows)}`);
        cases.push({ face, width, label, white_space: ws, markup, ...g });
      });
    }
  }
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, platform: process.platform, cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
