const { chromium } = require('playwright');
const fs = require('fs'), path = require('path');
const READ = `Array.prototype.map.call(document.querySelectorAll('img'), function (e) {
  var r = e.getBoundingClientRect(); var m = /i(\\d+)\\.png/.exec(e.currentSrc);
  return e.id + ':' + Math.round(r.width) + 'x' + Math.round(r.height) + ':p' + (m ? m[1] : 0) + ':n' + e.naturalWidth + 'x' + e.naturalHeight;
}).join(' ')`;
(async () => {
  const exe = process.env.CHROME;
  const dir = process.argv[2];
  const b = await chromium.launch({ headless: true, executablePath: exe });
  console.log('chrome', b.version());
  for (const dpr of [1, 2]) {
    const ctx = await b.newContext({ viewport: { width: 800, height: 600 }, deviceScaleFactor: dpr });
    const p = await ctx.newPage();
    const fetched = [];
    await p.route('http://fixture.test/**', route => {
      const f = path.join(dir, new URL(route.request().url()).pathname);
      fetched.push(path.basename(f));
      route.fulfill({ body: fs.readFileSync(f), contentType: f.endsWith('.png') ? 'image/png' : 'text/html' });
    });
    await p.goto('http://fixture.test/' + (process.argv[3] || 'img1_matrix.html'), { waitUntil: 'load' });
    await p.evaluate(() => window.scrollTo(0, 100000)); await p.waitForTimeout(500);
    console.log('dpr' + dpr, await p.evaluate(READ));
    await ctx.close();
  }
  await b.close();
})();
