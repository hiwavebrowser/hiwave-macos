// What the oracle's Chrome clicks on a page with `position: fixed` boxes
// after a scroll: the answers the engine tests in
// `crates/rustkit-engine/src/fixed_under_scroll_tests.rs` expect (Z D1, #681).
// The pages are the tests' own, in the tests' 200x100 view, scrolled by 50.
// usage: node tools/parity_oracle/fixed_scroll_click_log.mjs
import { chromium } from 'playwright';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const fixed = (href, z) =>
  `<a href='${href}' style='display:block;position:fixed;left:0;top:0;width:200px;height:20px;z-index:${z}'>x</a>`;
const under = "<a href='/under' style='display:block;height:300px'>under</a>";
// [name, body, points as [x, y] in the window]
const pages = [
  ['bar over a link',
    "<a href='/bar' style='display:block;position:fixed;left:0;top:0;width:200px;height:20px'>bar</a>" +
    "<div style='height:40px;background:none'></div><a href='/under' style='display:block;height:100px'>under</a>",
    [[20, 10], [20, 30]]],
  ['bar 60 down',
    "<a href='/bar' style='display:block;position:fixed;left:0;top:60px;width:200px;height:20px'>bar</a>" + under,
    [[20, 20], [20, 70]]],
  ['bar outside its parent',
    "<div style='height:300px;background:none'></div><div style='height:0'>" +
    "<a href='/bar' style='display:block;position:fixed;left:0;top:0;width:200px;height:20px'>bar</a></div>",
    [[20, 10]]],
  ['under a transformed ancestor',
    "<section style='transform:translate(0,0);height:300px'>" +
    "<a href='/bar' style='display:block;position:fixed;left:0;top:60px;width:200px;height:20px'>bar</a></section>",
    [[20, 20], [20, 70]]],
  ['two fixed, z 2 first', fixed('/dialog', 2) + fixed('/bar', 1) + under, [[20, 10]]],
  ['fixed with z -1', fixed('/behind', -1) + under, [[20, 10]]],
  ['hidden fixed',
    "<div style='position:fixed;left:0;top:0;width:200px;height:100px;visibility:hidden;background:none'></div>" + under,
    [[20, 10]]],
];
const events =
  "<div style='position:fixed;left:0;top:0;width:100px;height:20px;background:none'>" +
  "<button id=b style='display:block;width:100px;height:20px;border:0;padding:0;margin:0'></button></div>" +
  "<div style='position:fixed;left:100px;top:0;width:80px;height:20px;background:none'>" +
  "<input id=i style='display:block;width:80px;height:20px;border:0;padding:0;margin:0'></div>" +
  "<p id=p style='margin:0;height:300px'></p>" +
  "<script>var log = [];" +
  "['pointerdown','mousedown','pointerup','mouseup','click','mousemove'].forEach(function (type) {" +
  "document.addEventListener(type, function (e) { log.push(type + ':' + e.target.id + ':' + e.offsetY); });" +
  '});</script>';
const html = (body) =>
  '<html><head><style>body{margin:0} div{background:#000} #tall{height:1000px;background:none}</style></head>' +
  `<body>${body}<div id=tall></div></body></html>`;

const browser = await chromium.launch(getDeterministicLaunchOptions());
console.log('chrome ' + browser.version());
const page = await browser.newPage({ viewport: { width: 200, height: 100 } });
for (const [name, body, points] of pages) {
  await page.setContent(html(body));
  await page.evaluate(() => window.scrollTo(0, 50));
  const hits = await page.evaluate(
    (pts) => pts.map(([x, y]) => {
      const link = document.elementFromPoint(x, y)?.closest('a');
      return `(${x},${y}) ${link ? link.getAttribute('href') : 'no link'}`;
    }),
    points,
  );
  console.log(`## ${name}: ${hits.join(' | ')}`);
}
await page.setContent(html(events));
await page.evaluate(() => window.scrollTo(0, 50));
await page.mouse.move(20, 10);
await page.mouse.down();
await page.mouse.up();
console.log('## events on a fixed button: ' + (await page.evaluate(() => window.log.join(' '))));
await page.mouse.click(120, 8);
console.log('## focus after a click at (120,8): ' + (await page.evaluate(() => document.activeElement.id)));
await browser.close();
