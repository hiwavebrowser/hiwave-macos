// What the pinned Chrome computes for the text-decoration shorthand: its
// line, style and colour longhands and the shorthand read back, for a
// shorthand with a colour, a colour alone, a style, a thickness, the
// shorthand resetting an earlier longhand, and invalid values.
// Writes docs/diagnostics/2026-11-17/text_decoration_shorthand_cft148.json.
// usage: PARITY_CHROME_PATH=... node tools/parity_oracle/text_decoration_shorthand_log.mjs
import { chromium } from 'playwright';
import { writeFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const RECEIPT = join(dirname(fileURLToPath(import.meta.url)),
  '../../docs/diagnostics/2026-11-17/text_decoration_shorthand_cft148.json');

const browser = await chromium.launch({
  ...getDeterministicLaunchOptions(),
  executablePath: getPinnedExecutablePath({ required: true }),
});
if (browser.version() !== PIN) {
  throw new Error(`expected Chrome for Testing ${PIN}, got ${browser.version()}`);
}
const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
console.log('chrome ' + browser.version());
const cases = [];
const PROPS = ['text-decoration-line', 'text-decoration-style', 'text-decoration-color', 'text-decoration'];

async function probe(label, css) {
  await page.setContent('<!doctype html><style>body{margin:0}#t{color:rgb(0,0,255);font:16px sans-serif}' + css +
    '</style><div id=t>text</div>');
  const out = await page.evaluate((props) => {
    const cs = getComputedStyle(document.getElementById('t'));
    return Object.fromEntries(props.map((p) => [p, cs.getPropertyValue(p)]));
  }, PROPS);
  console.log(`${label}: ${PROPS.map((p) => out[p]).join(' | ')}`);
  cases.push({ label, css, ...out });
}

await probe('line and colour', '#t{text-decoration:underline red}');
await probe('colour first', '#t{text-decoration:red underline}');
await probe('line style colour', '#t{text-decoration:underline wavy rgb(0,128,0)}');
await probe('two lines and a colour', '#t{text-decoration:underline overline red}');
await probe('colour alone', '#t{text-decoration:red}');
await probe('colour alone after a line', '#t{text-decoration:underline}#t{text-decoration:red}');
await probe('style alone', '#t{text-decoration:dotted}');
await probe('with thickness', '#t{text-decoration:underline red 2px}');
await probe('shorthand resets an earlier colour', '#t{text-decoration-color:red;text-decoration:underline}');
await probe('shorthand resets an earlier style', '#t{text-decoration-style:wavy;text-decoration:underline red}');
await probe('none with a colour', '#t{text-decoration:none red}');
await probe('currentcolor', '#t{text-decoration:underline currentcolor}');
await probe('invalid token drops the declaration', '#t{text-decoration:underline}#t{text-decoration:overline bogus red}');
await probe('two colours is invalid', '#t{text-decoration:underline}#t{text-decoration:overline red blue}');
await probe('longhand after shorthand', '#t{text-decoration:underline red;text-decoration-color:rgb(0,128,0)}');

await browser.close();
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
