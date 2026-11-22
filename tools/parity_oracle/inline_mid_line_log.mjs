// Row INLINE-MID-LINE: an inline element (a span) whose text does not fit the rest of the current
// line starts on that line, after the content before it, and wraps from there (CSS2 9.4.2: an
// inline box is split across line boxes). RustKit moved the whole span to the next line when it
// did not fit (only a bare text run could start mid-line). The settings page's privacy notice is
// `<b>...</b> <span style="font-size:0.8rem; line-height:1.5">long text</span>`.
// Records the block's height, the span's fragments (getClientRects) and bounding rect, and the
// rects of the content before (#n) and after (#t) it. The same pages are the rustkit-engine test
// inline_mid_line_tests.
// Writes docs/diagnostics/2026-11-21/inline_mid_line_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/inline_mid_line_log.mjs
import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const RECEIPT = join(dirname(fileURLToPath(import.meta.url)), '../../docs/diagnostics/2026-11-21/inline_mid_line_cft148.json');
const TEXT = 'HiWave analytics are 100% local and private. We track only domain names, never full URLs, ' +
  'page titles, or search queries. All data stays on your device and is never sent to any server.';
const SMALL = 'font-size:12.8px;line-height:1.5';
// [label, block family, the block's inner HTML]
const CASES = [];
for (const family of ['Arial', "'Segoe UI'"]) {
  CASES.push([`${family}: span after text`, family, `<b id=n>Lead</b> <span id=s>${TEXT}</span>`]);
  CASES.push([`${family}: span after text, text after it`, family, `<b id=n>Lead</b> <span id=s>${TEXT}</span> <b id=t>tail</b>`]);
  CASES.push([`${family}: smaller span after text`, family, `<b id=n>Lead</b> <span id=s style="${SMALL}">${TEXT}</span>`]);
  CASES.push([`${family}: span after a bare text run`, family, `Lead in <span id=s>${TEXT}</span>`]);
  CASES.push([`${family}: span with a bold child after text`, family, `<b id=n>Lead</b> <span id=s>${TEXT.slice(0, 60)}<b>bold words</b>${TEXT.slice(60)}</span>`]);
}

const browser = await chromium.launch({
  ...getDeterministicLaunchOptions(),
  headless: true,
  executablePath: getPinnedExecutablePath({ required: true }),
});
const cases = [];
try {
  if (browser.version() !== PIN) throw new Error(`expected ${PIN}, got ${browser.version()}`);
  const page = await browser.newPage({ viewport: { width: 800, height: 400 } });
  for (const [label, family, inner] of CASES) {
    await page.setContent(`<!doctype html><style>body{margin:0}</style><div id=d style="font:16px ${family};width:500px">${inner}</div>`);
    const got = await page.evaluate(() => {
      const r = (e) => { if (!e) return null; const b = e.getBoundingClientRect(); return [b.x, b.y, b.width, b.height]; };
      const s = document.getElementById('s');
      return {
        block_height: document.getElementById('d').getBoundingClientRect().height,
        span: r(s),
        span_fragments: [...s.getClientRects()].map((b) => [b.x, b.y, b.width, b.height]),
        before: r(document.getElementById('n')),
        after: r(document.getElementById('t')),
      };
    });
    console.log(label, JSON.stringify(got));
    cases.push({ label, family, inner, ...got });
  }
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, platform: process.platform, text: TEXT, cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
