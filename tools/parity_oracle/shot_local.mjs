// Scratch helper (hub only): screenshot a local page with pinned Chrome, plus the
// computed style of every [id] element for the listed properties.
//   node shot_local.mjs <file.html> <out.png> <out.json> [prop,prop,...]
import { chromium } from 'playwright';
import { writeFileSync } from 'fs';
import { pathToFileURL } from 'url';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const [file, png, out, props = 'display'] = process.argv.slice(2);
const browser = await chromium.launch({ ...getDeterministicLaunchOptions(), headless: true });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
await page.goto(pathToFileURL(file).href, { waitUntil: 'load' });
await page.screenshot({ path: png });
const styles = await page.evaluate((names) => {
  const o = {};
  for (const e of document.querySelectorAll('[id]')) {
    const r = e.getBoundingClientRect();
    const cs = getComputedStyle(e);
    o[e.id] = { rect: [r.x, r.y, r.width, r.height], ...Object.fromEntries(names.map((n) => [n, cs.getPropertyValue(n)])) };
  }
  return o;
}, props.split(','));
writeFileSync(out, JSON.stringify(styles, null, 1));
await browser.close();
