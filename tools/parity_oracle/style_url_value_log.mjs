// SEC-STYLE-URL: which inline style values Chrome for Testing 148 keeps and which it refuses as holding a bad
// url, written through el.style.setProperty on a custom property (which accepts any value but a bad url, so
// the answer is the url rule alone) and through the style attribute. Each value is the CSS text itself.
// Output: docs/diagnostics/2026-11-17/style_url_value_chrome148.json. Run with PARITY_CHROME_PATH set.
import { chromium } from 'playwright';
const browser = await chromium.launch({ executablePath: process.env.PARITY_CHROME_PATH, headless: true });
const values = [
  'url(a b)', 'url(ok.png)', 'URL(a b)', 'none, url(a(b)',
  '\\75rl(a b)', '\\75 rl(a b)', 'u\\72l(a b)', 'ur\\6c(a b)', '\\55 RL(a b)', '\\55\\52\\4c(a b)',
  '\\75\\72\\6c(a b)', '\\000075rl(a b)', '\\00075 rl(a b)', 'u\\rl(a b)', '\\u\\r\\l(a b)', 'U\\52 L(a b)',
  '\\75rl(ok.png)', 'u\\72l(ok.png)', '\\75rx(a b)', 'x\\75rl(a b)', '-\\75rl(a b)', '\\-url(a b)',
  'f(\\75rl(a b))', '"\\75rl(a b)"', 'url(a\\ b)', '\\75rl(a\\ b)', 'u\\\nrl(a b)', '\\75rl(a"b)',
  '/* url(a b) */', 'url(a\u00a0b)', 'url( ok.png )', 'url(ok.png', 'url(a\\\nb)', 'url(a\u0001b)', 'url(a\tb)',
  '#url(a b)', '1url(a b)', 'url(a b', 'url(\ta b)', "url( 'a b' )", 'url(a)b(c d)',
];
const out = { chrome: browser.version(), property: '--x', cases: [] };
const page = await browser.newPage();
await page.setContent('<div id=a></div>');
for (const v of values) {
  const r = await page.evaluate((v) => {
    const el = document.getElementById('a');
    el.removeAttribute('style');
    el.style.setProperty('--x', 'before');
    el.style.setProperty('--x', v);
    const viaSet = el.style.getPropertyValue('--x');
    el.setAttribute('style', '--x: ' + v);
    const viaAttr = el.style.length ? el.style.getPropertyValue('--x') : null;
    return { kept_via_setProperty: viaSet !== 'before', value_via_setProperty: viaSet, kept_via_attribute: viaAttr !== null, value_via_attribute: viaAttr };
  }, v);
  out.cases.push({ css: v, ...r });
  console.error(JSON.stringify(v), JSON.stringify(r));
}
// Built-ins replaced by page script do not change the answer.
out.replaced_builtins = await page.evaluate(() => {
  const el = document.getElementById('a');
  String.prototype.substr = () => 'x'; String.prototype.toLowerCase = function () { return 'x'; };
  String.prototype.charAt = () => 'x'; RegExp.prototype.test = () => false; String.prototype.indexOf = () => -1;
  el.style.setProperty('--x', 'before');
  el.style.setProperty('--x', 'url(a b)');
  return el.style.getPropertyValue('--x');
});
console.log(JSON.stringify(out, null, 1));
await browser.close();
