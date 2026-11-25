// STRESS-SPEED (WINDOW-PATH-WALK): an event that reaches the Window. RustKit's dispatch asked the
// Window for `parentNode` and `nodeType` on every event (shadow retargeting and the body/frameset
// handler check), and each read fell through to named access on the Window object, a whole-document
// walk. Chrome's Window has neither property: they read `undefined`, or the element an id or name
// picks out. This records, for the same page, how many times each listener runs and what
// `window.parentNode` / `window.nodeType` read with and without such elements. The same page is
// the rustkit-engine test window_event_path_tests.
// Writes docs/diagnostics/2026-11-24/window_event_path_cft148.json.
//   PARITY_CHROME_PATH=<pinned Chrome for Testing> node tools/parity_oracle/window_event_path_log.mjs
import { chromium } from 'playwright';
import { writeFileSync, mkdirSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions, getPinnedExecutablePath } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const RECEIPT = join(dirname(fileURLToPath(import.meta.url)), '../../docs/diagnostics/2026-11-24/window_event_path_cft148.json');
export const SCRIPT = `
var n = { win: 0, doc: 0, body: 0, btn: 0, inline: 0 };
window.addEventListener('click', function () { n.win++; });
window.addEventListener('todo-add', function () { n.win++; });
document.addEventListener('click', function () { n.doc++; });
document.body.onclick = function () { n.body++; };
var b = document.getElementById('b');
b.addEventListener('todo-add', function () { n.btn++; });
for (var i = 0; i < 50; i++) {
  b.dispatchEvent(new MouseEvent('click', { bubbles: true }));
  b.dispatchEvent(new CustomEvent('todo-add', { bubbles: true, composed: true }));
}
var pn = window.parentNode, nt = window.nodeType;
[n.win, n.doc, n.body, n.btn, n.inline, pn === undefined ? 'undefined' : pn.id, nt === undefined ? 'undefined' : nt.id].join(' ');
`;
const PAGES = [
  ['plain', '<div id=a><button id=b onclick="n.inline++">x</button></div>'],
  ['an element with id=parentNode and one named nodeType', '<div id=parentNode></div><form name=nodeType id=nt></form><div id=a><button id=b onclick="n.inline++">x</button></div>'],
];
const browser = await chromium.launch({ ...getDeterministicLaunchOptions(), headless: true, executablePath: getPinnedExecutablePath({ required: true }) });
const cases = [];
try {
  if (browser.version() !== PIN) throw new Error(`expected ${PIN}, got ${browser.version()}`);
  const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
  for (const [label, body] of PAGES) {
    await page.setContent(`<!doctype html><body>${body}</body>`);
    const got = await page.evaluate(SCRIPT);
    console.log(label, got);
    cases.push({ label, body, got });
  }
} finally {
  await browser.close();
}
mkdirSync(dirname(RECEIPT), { recursive: true });
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, platform: process.platform, script: SCRIPT, cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
