// Chrome for Testing 148: the pixels of each SVG drawn into a 50x50 canvas (canvas_draw_svg.html).
// Writes canvas_draw_svg.chrome148.json: {chrome, pixels: {name: hex RGBA, 50*50*4 bytes}}.
// Run: PLAYWRIGHT=<the playwright module> CHROME=<the pinned Chrome for Testing binary> node <this file>
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
const fs = require('fs'), path = require('path');
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROME });
  const p = await b.newPage({ viewport: { width: 400, height: 400 }, deviceScaleFactor: 1 });
  await p.route('http://fixture.test/**', route => {
    const f = path.join(__dirname, new URL(route.request().url()).pathname);
    route.fulfill({ body: fs.readFileSync(f), contentType: f.endsWith('.svg') ? 'image/svg+xml' : 'text/html' });
  });
  await p.goto('http://fixture.test/canvas_draw_svg.html');
  await p.waitForFunction(() => window.__str7 !== null);
  const pixels = await p.evaluate(() => window.__str7);
  fs.writeFileSync(path.join(__dirname, 'canvas_draw_svg.chrome148.json'), JSON.stringify({ chrome: b.version(), pixels }, null, 1) + '\n');
  for (const [name, hex] of Object.entries(pixels)) {
    let opaque = 0, partial = 0;
    for (let i = 6; i < hex.length; i += 8) { const a = parseInt(hex.substr(i, 2), 16); if (a === 255) opaque++; else if (a > 0) partial++; }
    console.log(name, 'opaque', opaque, 'partial', partial);
  }
  await b.close();
})();
