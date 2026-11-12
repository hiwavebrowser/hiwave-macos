// Chrome for Testing 148: widths of w1..w6 and k1..k7 on css_container_query_inline_and_table.html.
// Run: PLAYWRIGHT=<the playwright module> CHROME=<the pinned Chrome for Testing binary> node <this file>
const { chromium } = require(process.env.PLAYWRIGHT || 'playwright');
const path = require('path');
(async () => {
  const b = await chromium.launch({ headless: true, executablePath: process.env.CHROME });
  const p = await b.newPage({ viewport: { width: 1280, height: 800 } });
  await p.goto('file://' + path.join(__dirname, 'css_container_query_inline_and_table.html'));
  const ids = ['w1', 'w2', 'w3', 'w4', 'w5', 'w6', 'k1', 'k2', 'k3', 'k4', 'k5', 'k6', 'k7'];
  const out = await p.evaluate(ids => ids.map(id => id + '=' + Math.round(document.getElementById(id).getBoundingClientRect().width)).join(' '), ids);
  console.log('chrome ' + b.version());
  console.log(out);
  await b.close();
})();
