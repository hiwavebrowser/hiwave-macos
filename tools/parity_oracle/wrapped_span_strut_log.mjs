// Row WRAPPED-SPAN-STRUT: a span whose text wraps over several lines, in a smaller font than its
// block (the settings page's privacy notice: `<span style="font-size: 0.8rem; line-height: 1.5">`
// after a block `<strong>`, in a 16px block). Every line box unites the block's strut with the
// span's own box on the baseline, so each line's baseline is the strut's ascent below the line
// top and the span's text sits that far down, not its own half-leading. Records the span's
// fragments (getClientRects), its bounding rect, the block's height, and the rect of a
// neighbour before or after it on the shared line. The same page is the rustkit-engine test
// wrapped_span_strut_tests.
// Writes docs/diagnostics/2026-11-21/wrapped_span_strut_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/wrapped_span_strut_log.mjs
import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const RECEIPT = join(dirname(fileURLToPath(import.meta.url)), '../../docs/diagnostics/2026-11-21/wrapped_span_strut_cft148.json');
const TEXT = 'HiWave analytics are 100% local and private. We track only domain names, never full URLs, ' +
  'page titles, or search queries. All data stays on your device and is never sent to any server.';
const SPAN = 'font-size:12.8px;line-height:1.5';
// [label, block family, the block's inner HTML]
const CASES = [];
for (const family of ['Arial', "'Segoe UI'"]) {
  CASES.push([`${family}: span alone after a block`, family, `<strong style="display:block">Notice</strong><span id=s style="${SPAN}">${TEXT}</span>`]);
  CASES.push([`${family}: span alone, normal line-height`, family, `<span id=s style="font-size:12.8px">${TEXT}</span>`]);
  CASES.push([`${family}: text before the span`, family, `<b id=n>Lead</b> <span id=s style="${SPAN}">${TEXT}</span>`]);
  CASES.push([`${family}: text after the span`, family, `<span id=s style="${SPAN}">${TEXT}</span> <b id=n>tail</b>`]);
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
      const r = (e) => { const b = e.getBoundingClientRect(); return [b.x, b.y, b.width, b.height]; };
      const s = document.getElementById('s');
      const n = document.getElementById('n');
      return {
        block_height: document.getElementById('d').getBoundingClientRect().height,
        span: r(s),
        span_fragments: [...s.getClientRects()].map((b) => [b.x, b.y, b.width, b.height]),
        neighbour: n ? r(n) : null,
      };
    });
    console.log(label, JSON.stringify(got));
    cases.push({ label, family, inner, ...got });
  }
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, platform: process.platform, text: TEXT, span_style: SPAN, cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
