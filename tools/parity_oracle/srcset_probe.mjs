// CH2.E54b: what a Chromium does for <img srcset/sizes> and <picture>.
//
//   node tools/parity_oracle/srcset_probe.mjs            # Playwright's bundled Chromium
//   CHROME_PATH=/path/to/chrome node tools/parity_oracle/srcset_probe.mjs   # e.g. Chrome for Testing 148
//
// Serves four solid PNGs (a=100x50, b=200x100, c=400x200, d=800x400) and a case page from a
// temp dir, loads each markup at viewport 800x600 with device pixel ratio 1 and 2, and prints
// currentSrc / natural size / shown size. The expected table (Chromium 143.0.7499.4) is in
// docs/diagnostics/2026-11-10/e54b_srcset_chromium.txt.
import { createRequire } from 'module';
import fs from 'fs';
import os from 'os';
import path from 'path';
import http from 'http';
import zlib from 'zlib';
import { fileURLToPath } from 'url';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, 'x.js'));
const { chromium } = require('playwright');

// A solid-colour PNG, no dependencies.
function png(width, height) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (buf) => {
    let c = 0xffffffff;
    for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type), data]);
    const sum = Buffer.alloc(4); sum.writeUInt32BE(crc(body));
    return Buffer.concat([len, body, sum]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0); ihdr.writeUInt32BE(height, 4); ihdr[8] = 8; ihdr[9] = 2;
  const row = Buffer.concat([Buffer.from([0]), Buffer.from(Array.from({ length: width }, () => [200, 50, 50]).flat())]);
  const raw = Buffer.concat(Array.from({ length: height }, () => row));
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', ihdr), chunk('IDAT', zlib.deflateSync(raw)), chunk('IEND', Buffer.alloc(0)),
  ]);
}

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'srcset-probe-'));
for (const [n, w] of Object.entries({ a: 100, b: 200, c: 400, d: 800 })) fs.writeFileSync(path.join(dir, `${n}.png`), png(w, w / 2));

const W4 = 'a.png 100w, b.png 200w, c.png 400w, d.png 800w';
const cases = [
  ['x-descriptors', '<img id=i srcset="a.png 1x, b.png 2x, c.png 4x" alt="">'],
  ['x-1.5-3', '<img id=i srcset="a.png 1.5x, b.png 3x" alt="">'],
  ['w-sizes-300px', `<img id=i srcset="${W4}" sizes="300px" alt="">`],
  ['w-sizes-50vw', `<img id=i srcset="${W4}" sizes="50vw" alt="">`],
  ['w-sizes-media', `<img id=i srcset="${W4}" sizes="(max-width: 500px) 100px, 600px" alt="">`],
  ['w-no-sizes', `<img id=i srcset="${W4}" alt="">`],
  ['w-sizes-calc', `<img id=i srcset="${W4}" sizes="calc(50vw - 100px)" alt="">`],
  ['src-plus-srcset-x', '<img id=i src="a.png" srcset="b.png 2x" alt="">'],
  ['src-plus-srcset-w', '<img id=i src="a.png" srcset="b.png 200w, c.png 400w" sizes="200px" alt="">'],
  ['picture-media-hit', '<picture><source media="(max-width: 900px)" srcset="b.png"><img id=i src="c.png" alt=""></picture>'],
  ['picture-media-miss', '<picture><source media="(max-width: 500px)" srcset="b.png"><img id=i src="c.png" alt=""></picture>'],
  ['picture-type-unsupported', '<picture><source type="image/x-nonsense" srcset="b.png"><img id=i src="c.png" alt=""></picture>'],
  ['picture-type-png', '<picture><source type="image/png" srcset="b.png"><img id=i src="c.png" alt=""></picture>'],
  ['picture-first-match-wins', '<picture><source srcset="a.png"><source srcset="b.png"><img id=i src="c.png" alt=""></picture>'],
  ['picture-source-w-sizes', '<picture><source media="(min-width: 400px)" srcset="a.png 100w, b.png 200w, d.png 800w" sizes="400px"><img id=i src="c.png" alt=""></picture>'],
  ['x-with-css-width', '<img id=i srcset="a.png 1x, b.png 2x" style="width:50px" alt="">'],
  ['x-2-only', '<img id=i srcset="b.png 2x" alt="">'],
];

const srv = http.createServer((req, res) => {
  const f = path.join(dir, req.url.split('?')[0].replace(/^\//, ''));
  if (fs.existsSync(f) && fs.statSync(f).isFile()) {
    res.writeHead(200, { 'Content-Type': f.endsWith('.png') ? 'image/png' : 'text/html' });
    res.end(fs.readFileSync(f));
  } else { res.writeHead(404); res.end(); }
});
await new Promise((r) => srv.listen(0, '127.0.0.1', r));
const port = srv.address().port;
const browser = await chromium.launch({ args: ['--no-sandbox'], ...(process.env.CHROME_PATH ? { executablePath: process.env.CHROME_PATH } : {}) });
console.log('chromium', browser.version());
const rows = [];
for (const dpr of [1, 2]) {
  const ctx = await browser.newContext({ viewport: { width: 800, height: 600 }, deviceScaleFactor: dpr });
  for (const [name, body] of cases) {
    const page = await ctx.newPage();
    fs.writeFileSync(path.join(dir, 'case.html'), `<!doctype html><body style="margin:0">${body}</body>`);
    await page.goto(`http://127.0.0.1:${port}/case.html`, { waitUntil: 'load' });
    await page.waitForFunction(() => document.getElementById('i').complete);
    const r = await page.evaluate(() => {
      const i = document.getElementById('i');
      return { cur: i.currentSrc.split('/').pop(), nat: `${i.naturalWidth}x${i.naturalHeight}`, shown: `${i.width}x${i.height}` };
    });
    rows.push({ dpr, name, ...r });
    await page.close();
  }
  await ctx.close();
}
await browser.close();
srv.close();
fs.rmSync(dir, { recursive: true, force: true });
const w = Math.max(...rows.map((r) => r.name.length));
for (const r of rows) console.log(`dpr=${r.dpr} ${r.name.padEnd(w)} cur=${r.cur.padEnd(6)} natural=${r.nat.padEnd(8)} shown=${r.shown}`);
