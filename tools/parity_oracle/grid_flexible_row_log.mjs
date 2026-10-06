// How tall is a flexible (fr) row in a grid whose height is auto?
//
// The pinned Chromium lays out each page in grid_flexible_row_cases.json and
// reports, for every element with an id, the top and height of its border
// box as "id:y:height". The engine is tested against these strings
// (crates/rustkit-engine, grid_flexible_row_tests).
//
//   node grid_flexible_row_log.mjs            print
//   node grid_flexible_row_log.mjs --write    print, and store in the case file
import { readFileSync, writeFileSync } from 'node:fs';
import { chromium } from 'playwright';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const FILE = new URL('./grid_flexible_row_cases.json', import.meta.url);
// The same expression the engine test evaluates.
export const BOXES = `Array.prototype.map.call(document.querySelectorAll('[id]'), function (e) {
  var r = e.getBoundingClientRect();
  return e.id + ':' + Math.round(r.top) + ':' + Math.round(r.height);
}).join(' ')`;

const data = JSON.parse(readFileSync(FILE, 'utf8'));
const [width, height] = data.viewport;
const browser = await chromium.launch(getDeterministicLaunchOptions());
console.log('chrome ' + browser.version());
for (const c of data.cases) {
  const page = await browser.newPage({ viewport: { width, height } });
  await page.setContent(c.html);
  c.chrome_boxes = await page.evaluate(BOXES);
  console.log(c.name + '  ' + c.chrome_boxes);
  await page.close();
}
if (process.argv.includes('--write')) {
  data.chrome = browser.version();
  writeFileSync(FILE, JSON.stringify(data, null, 1) + '\n');
}
await browser.close();
