// Scratch helper (hub only): border-box rect of every element with an id, from pinned Chrome.
//   node rects_local.mjs <file.html> <out.json> [width height]
import { chromium } from 'playwright';
import { writeFileSync } from 'fs';
import { pathToFileURL } from 'url';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const [file, out, w = '1280', h = '800'] = process.argv.slice(2);
const browser = await chromium.launch({ ...getDeterministicLaunchOptions(), headless: true });
const page = await browser.newPage({ viewport: { width: +w, height: +h } });
await page.goto(pathToFileURL(file).href, { waitUntil: 'load' });
const rects = await page.evaluate(() => {
  const o = {};
  for (const e of document.querySelectorAll('[id]')) {
    const r = e.getBoundingClientRect();
    const cs = getComputedStyle(e);
    o[e.id] = [r.x, r.y, r.width, r.height, cs.display];
  }
  return o;
});
writeFileSync(out, JSON.stringify(rects));
await browser.close();
