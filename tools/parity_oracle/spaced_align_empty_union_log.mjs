// LETTER-SPACING-BREAK follow-ups (#1491 R1 nits 2 and 3), measured in Chrome before RustKit:
//   align: a wrapped, letter-spaced paragraph under text-align center / right. Each line's
//          characters (white space dropped) and the left edge of its first character, from a
//          Range per character. A line is aligned by its SPACED width.
//   union: a span whose text is only preserved newlines, at a line end. Its
//          getBoundingClientRect() (x, y, width, height): every fragment is empty.
// Ahem 20px/1 (the WPT face). The same rows are the rustkit-engine test
// letter_spacing_edges_tests.
// Writes docs/diagnostics/2026-11-28/spaced_align_empty_union_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/spaced_align_empty_union_log.mjs
import { chromium } from 'playwright';
import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const HERE = dirname(fileURLToPath(import.meta.url));
const RECEIPT = join(HERE, '../../docs/diagnostics/2026-11-28/spaced_align_empty_union_cft148.json');
const AHEM = readFileSync(join(HERE, '../../crates/rustkit-text/tests/fixtures/Ahem.ttf')).toString('base64');
const FACE = `@font-face{font-family:Ahem;src:url(data:font/ttf;base64,${AHEM})}`;
const NL = String.fromCharCode(10);
const STYLE = 'body{margin:0} p{margin:0;font:20px/1 Ahem;width:200px}';

// (label, paragraph style, text): the paragraph's lines and each line's left edge.
const ALIGN = [
  ['centered, letter-spacing 0.5em', 'text-align:center;letter-spacing:0.5em', '1 2 3 4 5 6'],
  ['right-aligned, letter-spacing 0.5em', 'text-align:right;letter-spacing:0.5em', '1 2 3 4 5 6'],
  ['centered, word-spacing 1em', 'text-align:center;word-spacing:1em', '1 2 3 4 5 6 7'],
  ['centered, no spacing (control)', 'text-align:center', '1 2 3 4 5 6 7 8 9 1 2'],
];
// (label, paragraph markup with a span #s): the span's rect. Laid out 100px wide, pre-wrap.
const UNION = [
  ['a span holding one newline, at a line end', `1 2 3<span id=s>${NL}</span>6 7 8`],
  ['a span holding two newlines, at a line end', `1 2 3<span id=s>${NL}${NL}</span>6 7 8`],
];

const browser = await chromium.launch({
  ...getDeterministicLaunchOptions(),
  headless: true,
  executablePath: getPinnedExecutablePath({ required: true }),
});
const align = [];
const union = [];
try {
  if (browser.version() !== PIN) throw new Error(`expected ${PIN}, got ${browser.version()}`);
  const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
  for (const [label, style, text] of ALIGN) {
    await page.setContent(`<!doctype html><style>${FACE} ${STYLE}</style><p id=p style="${style}">${text}</p>`);
    await page.evaluate(() => document.fonts.ready);
    const got = await page.evaluate(() => {
      const p = document.getElementById('p');
      const lines = new Map();
      const t = p.firstChild;
      for (let k = 0; k < t.data.length; k++) {
        if (/\s/.test(t.data[k])) continue;
        const r = document.createRange();
        r.setStart(t, k);
        r.setEnd(t, k + 1);
        const rect = r.getClientRects()[0];
        const top = Math.round(rect.top);
        const line = lines.get(top) || { chars: '', left: rect.left };
        line.chars += t.data[k];
        line.left = Math.min(line.left, rect.left);
        lines.set(top, line);
      }
      return {
        height: p.getBoundingClientRect().height,
        lines: [...lines.entries()].sort((a, b) => a[0] - b[0]).map((e) => e[1]),
      };
    });
    align.push({ label, style, text, ...got });
  }
  for (const [label, markup] of UNION) {
    await page.setContent(
      `<!doctype html><style>${FACE} ${STYLE}</style><p id=p style="white-space:pre-wrap;width:100px">${markup}</p>`,
    );
    await page.evaluate(() => document.fonts.ready);
    const got = await page.evaluate(() => {
      const r = document.getElementById('s').getBoundingClientRect();
      return {
        height_p: document.getElementById('p').getBoundingClientRect().height,
        span: { x: r.x, y: r.y, width: r.width, height: r.height },
      };
    });
    union.push({ label, markup, ...got });
  }
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(
  RECEIPT,
  JSON.stringify({ chrome: PIN, oracle: 'tools/parity_oracle/spaced_align_empty_union_log.mjs', style: STYLE, align, union }, null, 2) + '\n',
);
console.log(JSON.stringify({ align, union }, null, 1));
