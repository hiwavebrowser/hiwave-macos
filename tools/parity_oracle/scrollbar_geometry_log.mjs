// CH2.E7: Chrome's styled-scrollbar geometry, for the RustKit engine tests
// in crates/rustkit-engine/src/styled_scrollbar_tests.rs.
// node tools/parity_oracle/scrollbar_geometry_log.mjs
import { chromium } from 'playwright';
import { PNG } from 'pngjs';

const CASES = {
  webkit6: `<style>body{margin:0}#s{width:300px;height:200px;overflow:auto;background:#fff}
    #s::-webkit-scrollbar{width:6px} #s::-webkit-scrollbar-track{background:rgb(0,0,255)}
    #s::-webkit-scrollbar-thumb{background:rgb(255,0,0);border-radius:3px}
    #c{height:600px}</style><div id=s><div id=c></div></div>`,
  none: `<style>body{margin:0}#s{width:300px;height:200px;overflow:auto;scrollbar-width:none}
    #s::-webkit-scrollbar{width:6px}#c{height:600px}</style><div id=s><div id=c></div></div>`,
  displaynone: `<style>body{margin:0}#s{width:300px;height:200px;overflow:auto}
    #s::-webkit-scrollbar{display:none}#c{height:600px}</style><div id=s><div id=c></div></div>`,
  thin: `<style>body{margin:0}#s{width:300px;height:200px;overflow:auto;scrollbar-width:thin;scrollbar-color:rgb(255,0,0) rgb(0,0,255)}
    #c{height:600px}</style><div id=s><div id=c></div></div>`,
  scroll_short: `<style>body{margin:0}#s{width:300px;height:200px;overflow:scroll}
    #s::-webkit-scrollbar{width:6px;height:6px}#s::-webkit-scrollbar-thumb{background:red}#c{height:50px}</style><div id=s><div id=c></div></div>`,
  root: `<style>body{margin:0}::-webkit-scrollbar{width:6px}::-webkit-scrollbar-thumb{background:rgb(255,0,0)}
    #c{height:2400px}</style><div id=c></div>`,
};

const browser = await chromium.launch({ ignoreDefaultArgs: ['--hide-scrollbars'], ...(process.env.PW_CHROMIUM ? { executablePath: process.env.PW_CHROMIUM } : {}) });
const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
for (const [name, html] of Object.entries(CASES)) {
  await page.setContent(html);
  const geo = await page.evaluate(() => {
    const s = document.getElementById('s') || document.documentElement;
    const c = document.getElementById('c');
    return { clientWidth: s.clientWidth, offsetWidth: s.offsetWidth, cWidth: c.getBoundingClientRect().width,
      scrollHeight: s.scrollHeight, sw: getComputedStyle(s).scrollbarWidth, w: getComputedStyle(s).width };
  });
  // Find the red thumb's vertical extent in column x.
  const thumbSpan = async (x) => {
    const png = PNG.sync.read(await page.screenshot());
    let top = -1, bottom = -1;
    for (let y = 0; y < png.height; y++) {
      const i = (y * png.width + x) * 4;
      const red = png.data[i] > 200 && png.data[i + 1] < 60 && png.data[i + 2] < 60;
      if (red) { if (top < 0) top = y; bottom = y; }
    }
    return [top, bottom + 1];
  };
  const x = name === 'root' ? 797 : 297;
  const before = await thumbSpan(x);
  if (name === 'root') await page.evaluate(() => window.scrollTo(0, 600));
  else await page.evaluate(() => { const s = document.getElementById('s'); if (s) s.scrollTop = 200; });
  const after = await thumbSpan(x);
  console.log(name, JSON.stringify(geo), 'thumb', before, 'after-scroll', after);
}
// Drag: 50px on the thumb of webkit6.
await page.setContent(CASES.webkit6);
for (const steps of [1, 10]) {
  await page.setContent(CASES.webkit6);
  await page.mouse.move(297, 10); await page.mouse.down(); await page.mouse.move(297, 60, { steps }); await page.waitForTimeout(300);
  console.log('drag50 steps', steps, 'scrollTop', await page.evaluate(() => document.getElementById('s').scrollTop));
  await page.mouse.up();
}
await page.setContent(CASES.webkit6);
await page.mouse.move(297, 150); await page.mouse.down(); await page.waitForTimeout(60); await page.mouse.up(); await page.waitForTimeout(800);
console.log('track-click scrollTop', await page.evaluate(() => document.getElementById('s').scrollTop));
await browser.close();
