// Headless CfT 148: heights of the two column-flex items, before and after the edit.
// Run: PLAYWRIGHT=<path to the playwright module> CHROME=<the pinned Chrome for Testing 148 binary> node flex_col_shrink_probe.js
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
const exe = process.env.CHROME;
const cases = {
  plain: "<style>body{margin:0;height:500px;display:flex;flex-direction:column} .i{height:300px}</style><div class=i><div id=t style='height:320px'></div></div><div class=i><div style='height:100px'></div></div>",
  padded: "<style>body{margin:0;height:500px;display:flex;flex-direction:column} .i{height:300px;padding:1px 0}</style><div class=i><div id=t style='height:320px'></div></div><div class=i><div style='height:100px'></div></div>",
};
(async () => {
  const browser = await chromium.launch({ headless: true, executablePath: exe });
  const page = await browser.newPage({ viewport: { width: 400, height: 600 } });
  console.log('version', browser.version());
  for (const [name, html] of Object.entries(cases)) {
    await page.setContent('<!doctype html>' + html);
    const read = () => page.evaluate(() => [...document.querySelectorAll('.i')].map(e => e.getBoundingClientRect().height));
    const before = await read();
    await page.evaluate(() => { document.getElementById('t').style.height = '150px'; });
    const after = await read();
    console.log(name, 'before', JSON.stringify(before), 'after', JSON.stringify(after));
  }
  await browser.close();
})();
