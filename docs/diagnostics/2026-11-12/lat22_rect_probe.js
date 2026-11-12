// Run: PLAYWRIGHT=<path to the playwright module> CHROME=<the pinned Chrome for Testing 148 binary> node lat22_rect_probe.js
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROME });
  const page = await browser.newPage({ viewport: { width: 1024, height: 768 } });
  console.log('version', browser.version());
  const html = "<!doctype html><style>body{margin:0;font:12px sans-serif} #a{width:50px;height:50px;background:green}</style><div id=a></div>";
  await page.setContent(html);
  console.log('moving width', await page.evaluate(() => {
    const a = document.getElementById('a');
    const an = a.animate([{ width: '50px' }, { width: '250px' }], { duration: 1000 }); an.pause(); an.currentTime = 500;
    return a.getBoundingClientRect().width;
  }));
  await page.setContent(html);
  console.log('transform with unchanging width', await page.evaluate(() => {
    const a = document.getElementById('a');
    const an = a.animate([{ transform: 'translateX(0px)', width: '50px' }, { transform: 'translateX(300px)', width: '50.000001px' }], { duration: 1000 }); an.pause(); an.currentTime = 500;
    const r = a.getBoundingClientRect(); return r.left + '/' + r.width;
  }));
  await page.setContent(html);
  console.log('unchanging width loop', await page.evaluate(() => {
    const a = document.getElementById('a');
    a.animate([{ width: '50px' }, { width: '50.000001px' }], { duration: 4000, iterations: Infinity });
    return a.getBoundingClientRect().width;
  }));
  await browser.close();
})();
