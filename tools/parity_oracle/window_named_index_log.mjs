// WINDOW-NAMED-INDEX: named access on the Window object (HTML 7.2.2.3) answered from an index kept
// until the DOM changes, instead of a walk of the whole document per lookup. The answers must be
// Chrome's, before and after each kind of change the index has to notice. Records, on the page of
// rustkit-bindings' web_named_access_tests, a sequence of reads around mutations.
// Writes docs/diagnostics/2026-11-24/window_named_index_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/window_named_index_log.mjs
import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const RECEIPT = join(dirname(fileURLToPath(import.meta.url)), '../../docs/diagnostics/2026-11-24/window_named_index_cft148.json');
const PAGE = '<!DOCTYPE html><html><body><div id=target>hello</div><form name=signup></form><img name=logo src=a.png>' +
  '<span name=notexposed></span><p id=dup>a</p><p id=dup>b</p><img id=both name=both><embed name=both></body></html>';
export const SCRIPT = `
function d(v) { return v === undefined ? 'undefined' : v instanceof HTMLCollection ? 'C' + v.length : v.localName + (v.id ? '#' + v.id : ''); }
var out = [];
out.push(d(window.target), d(window.signup), d(window.notexposed), d(window.dup), d(window.both), d(window.missing));
var e = document.createElement('div'); e.id = 'late'; document.body.appendChild(e);
out.push(d(window.late));
e.id = 'renamed'; out.push(d(window.late), d(window.renamed));
e.remove(); out.push(d(window.renamed));
document.getElementById('target').setAttribute('id', 'moved'); out.push(d(window.target), d(window.moved));
var f = document.createElement('form'); f.setAttribute('name', 'late2'); document.body.appendChild(f); out.push(d(window.late2));
f.setAttribute('name', 'late3'); out.push(d(window.late2), d(window.late3));
var s = document.createElement('span'); s.setAttribute('name', 'late4'); document.body.appendChild(s); out.push(d(window.late4));
document.body.insertAdjacentHTML('beforeend', '<p id=dup>c</p>'); out.push(d(window.dup));
var t = document.createElement('div'); t.id = 'kid'; var p = document.createElement('section'); p.appendChild(t); out.push(d(window.kid)); document.body.appendChild(p); out.push(d(window.kid));
out.join(' ');
`;
const browser = await chromium.launch({ ...getDeterministicLaunchOptions(), headless: true, executablePath: getPinnedExecutablePath({ required: true }) });
let got;
try {
  if (browser.version() !== PIN) throw new Error(`expected ${PIN}, got ${browser.version()}`);
  const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
  await page.route('**/a.png', (r) => r.fulfill({ status: 404, body: '' }));
  await page.setContent(PAGE);
  got = await page.evaluate((s) => (0, eval)(s), SCRIPT);
  console.log(got);
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, platform: process.platform, page: PAGE, script: SCRIPT, got }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
