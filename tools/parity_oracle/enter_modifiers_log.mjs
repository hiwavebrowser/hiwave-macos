// CH2.E37: what the oracle's Chrome does with Enter and its modifiers in a
// textarea whose page cancels a bare Enter (a chat box) and in a form field.
// The log the engine tests `enter_keydown_carries_its_modifiers_and_a_cancel_
// types_nothing` and `enter_in_a_textarea_types_a_newline_and_fires_one_keydown`
// expect.
// usage: node tools/parity_oracle/enter_modifiers_log.mjs
import { chromium } from 'playwright';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const PAGE = '<html><body style="margin:0">'
  + '<textarea id="t" style="display:block;width:200px;height:40px"></textarea>'
  + '<form id="f" action="/search"><input id="q" name="q" style="display:block;width:200px;height:30px"></form>'
  + '<textarea id="u" style="display:block;width:200px;height:40px"></textarea>'
  + '</body></html>';

const browser = await chromium.launch(getDeterministicLaunchOptions());
const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
await page.setContent(PAGE);
await page.evaluate(() => {
  window.log = [];
  document.addEventListener('keydown', (e) => {
    log.push(e.key + ':' + e.target.id + ':' + (e.shiftKey ? 'S' : '') + (e.ctrlKey ? 'C' : '') + (e.altKey ? 'A' : ''));
    if (e.key === 'Enter' && e.target.id === 't' && !e.shiftKey) e.preventDefault();
  });
  document.getElementById('f').addEventListener('submit', (e) => {
    log.push('submit:' + new URLSearchParams(new FormData(e.target)).toString());
    e.preventDefault();
  });
});
const step = async (label, fn) => {
  await page.evaluate(() => { window.log.length = 0; });
  await fn();
  const out = await page.evaluate(() => window.log.join(' ')
    + ' | t=' + JSON.stringify(document.getElementById('t').value)
    + ' u=' + JSON.stringify(document.getElementById('u').value));
  console.log('## ' + label + '\n' + out);
};
console.log('chrome ' + browser.version());
await page.click('#t');
await step('Shift+Enter in #t', () => page.keyboard.press('Shift+Enter'));
await step('Enter in #t (cancelled)', () => page.keyboard.press('Enter'));
await step('Control+Alt+Enter in #t', () => page.keyboard.press('Control+Alt+Enter'));
await page.click('#q');
await step('Shift+Enter in #q', () => page.keyboard.press('Shift+Enter'));
await page.click('#u');
await step('a Shift+Enter b in #u', async () => {
  await page.keyboard.press('a');
  await page.keyboard.press('Shift+Enter');
  await page.keyboard.press('b');
});
await step('Enter Enter in #u', async () => {
  await page.keyboard.press('Enter');
  await page.keyboard.press('Enter');
});
await browser.close();
