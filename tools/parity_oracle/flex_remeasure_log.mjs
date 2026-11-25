// FLEX-REMEASURE: nested flex containers laid out once per pass (LAT21's memo keyed so the block
// pre-pass and the flex pass of an item share a result, and admitting boxes that share an empty
// float context). The geometry must not move: these pages are nested stretch / flex / percent
// shapes, including Speedometer Editor-TipTap's (body flex > column flex > flex:1 editor with
// padding > paragraphs). Records each #id's border box. The same pages are the rustkit-engine test
// flex_remeasure_tests.
// Writes docs/diagnostics/2026-11-24/flex_remeasure_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/flex_remeasure_log.mjs
import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const RECEIPT = join(dirname(fileURLToPath(import.meta.url)), '../../docs/diagnostics/2026-11-24/flex_remeasure_cft148.json');
const PARA = (n) => Array.from({ length: n }, (_, i) => `<p id=p${i}>Paragraph ${i}: ${'word '.repeat(20 + (i * 7) % 40)}</p>`).join('');
export const PAGES = [
  ['editor', 'html,body{margin:0;height:100%} body{display:flex;padding:5px;box-sizing:border-box;font:13px/1.5 Arial} ' +
    '#app{flex:1;display:flex;flex-direction:column} #controls{display:flex;padding-bottom:5px} ' +
    '#editor{flex:1;overflow:auto;padding:2px 5px} p{margin:0 0 8px}',
    `<div id=app><div id=controls><button id=b>Create</button></div><div id=editor><div id=pm>${PARA(30)}</div></div></div>`],
  ['row stretch, percent child', 'body{margin:0;font:13px/1.5 Arial} #row{display:flex;height:200px} #a{flex:1;padding:4px} ' +
    '#b{flex:2;display:flex;flex-direction:column} #b1{height:50%} #b2{flex:1;padding:3px}',
    `<div id=row><div id=a>${PARA(3)}</div><div id=b><div id=b1>half</div><div id=b2>${PARA(2)}</div></div></div>`],
  ['column flex, basis 0, nested row', 'body{margin:0;font:13px/1.5 Arial} #col{display:flex;flex-direction:column;height:400px} ' +
    '#top{flex:0 0 auto;padding:6px} #mid{flex:1 1 0;display:flex;min-height:0} #m1{flex:1;overflow:auto;padding:2px} #m2{width:120px}',
    `<div id=col><div id=top>Top bar</div><div id=mid><div id=m1>${PARA(12)}</div><div id=m2>side</div></div></div>`],
  ['nested flex with a margin chain', 'body{margin:0;font:13px/1.5 Arial} #o{display:flex} #i{flex:1;display:flex;flex-direction:column} ' +
    '#k{margin:10px 0} #k p{margin:12px 0}',
    `<div id=o><div id=i><div id=k>${PARA(4)}</div></div></div>`],
];

const browser = await chromium.launch({ ...getDeterministicLaunchOptions(), headless: true, executablePath: getPinnedExecutablePath({ required: true }) });
const cases = [];
try {
  if (browser.version() !== PIN) throw new Error(`expected ${PIN}, got ${browser.version()}`);
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  for (const [label, css, body] of PAGES) {
    await page.setContent(`<!doctype html><style>${css}</style>${body}`);
    const rects = await page.evaluate(() => Object.fromEntries([...document.querySelectorAll('[id]')]
      .filter((e) => !/^p\d+$/.test(e.id))
      .map((e) => { const r = e.getBoundingClientRect(); return [e.id, [r.x, r.y, r.width, r.height]]; })));
    console.log(label, JSON.stringify(rects));
    cases.push({ label, css, body, rects });
  }
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, platform: process.platform, cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
