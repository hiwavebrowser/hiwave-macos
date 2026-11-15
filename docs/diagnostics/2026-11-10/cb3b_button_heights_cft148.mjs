// CH2.CB3b: heights of a <button style=font-size:16px>Button</button> for each combination of the three
// declarations that decide it (line-height, vertical padding, vertical border), in Chrome for Testing 148.
//   CHROME_PATH=<path to CfT 148 chrome> node cb3b_button_heights_cft148.mjs   (playwright from tools/parity_oracle/node_modules)
// Output on Linux and on the Mac (Atlas, 2026-11-10) agree: see cb3b_button_heights_cft148.txt.
import { createRequire } from 'module';
import path from 'path';
import { fileURLToPath } from 'url';
const here = path.dirname(fileURLToPath(import.meta.url));
const { chromium } = createRequire(path.join(here, '../../../tools/parity_oracle/x.js'))('playwright');
const b = await chromium.launch({ args: ['--no-sandbox'], ...(process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {}) });
const p = await b.newPage({ viewport: { width: 800, height: 600 } });
const variants = [
  ['default (UA only)', ''],
  ['line-height:1 only', 'line-height:1'],
  ['padding:0 only', 'padding:0'],
  ['border:none only', 'border:none'],
  ['padding:0; border:none', 'padding:0;border:none'],
  ['padding:0; border:none; line-height:1', 'padding:0;border:none;line-height:1'],
  ['line-height:1; padding:0', 'line-height:1;padding:0'],
  ['line-height:1; border:none', 'line-height:1;border:none'],
  ['padding:4px 8px', 'padding:4px 8px'],
];
const fonts = [['font-family default (UA)', ''], ['font-family:Arial', 'font-family:Arial']];
console.log('Chrome', b.version(), ' font-size:16px, text "Button", viewport 800x600');
for (const [fl, ff] of fonts) {
  console.log('\n== ' + fl);
  console.log('variant'.padEnd(42) + ' height   width   padTB(computed)   borderTB(computed)  line-height(computed)  font');
  for (const [label, css] of variants) {
    await p.setContent(`<!doctype html><body style="margin:0"><button id=b style="font-size:16px;${ff};${css}">Button</button></body>`);
    const r = await p.evaluate(() => { const e = document.getElementById('b'); const r = e.getBoundingClientRect(); const c = getComputedStyle(e);
      return { h: r.height, w: r.width, pt: c.paddingTop, pb: c.paddingBottom, bt: c.borderTopWidth, bb: c.borderBottomWidth, lh: c.lineHeight, ff: c.fontFamily.split(',')[0] }; });
    console.log(label.padEnd(42), String(r.h).padEnd(8), String(r.w).padEnd(7), `${r.pt}/${r.pb}`.padEnd(17), `${r.bt}/${r.bb}`.padEnd(19), r.lh.padEnd(22), r.ff);
  }
}
await b.close();
