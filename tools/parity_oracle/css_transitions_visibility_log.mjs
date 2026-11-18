// What the pinned Chrome reports for transitions of group E of the
// CSS-transitions brief (visibility): the value at currentTime 50
// of a 100ms linear transition (paused and seeked, no timers), which
// transitions run, and their events once finished.
// Writes docs/diagnostics/2026-11-17/css_transitions_visibility_cft148.json.
// usage: PARITY_CHROME_PATH=... node tools/parity_oracle/css_transitions_visibility_log.mjs
import { chromium } from 'playwright';
import { writeFileSync } from 'fs';
import { dirname, join } from 'path';
import { fileURLToPath } from 'url';
import { getDeterministicLaunchOptions } from './deterministic.mjs';

const PIN = '148.0.7778.216';
const RECEIPT = join(dirname(fileURLToPath(import.meta.url)),
  '../../docs/diagnostics/2026-11-17/css_transitions_visibility_cft148.json');

const browser = await chromium.launch(getDeterministicLaunchOptions({ requirePinned: true }));
if (browser.version() !== PIN) {
  throw new Error(`expected Chrome for Testing ${PIN}, got ${browser.version()}`);
}
const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
console.log('chrome ' + browser.version());
const cases = [];

async function probe(label, css, read) {
  await page.setContent('<!doctype html><style>body{margin:0}#h{width:100px;height:50px;font:16px sans-serif;' +
    'background:linear-gradient(red,blue) no-repeat;transition:all 100ms linear}' + css + '</style><div id=h>text</div>');
  await page.evaluate(() => document.body.offsetWidth);
  const out = await page.evaluate(async (read) => {
    const el = document.getElementById('h');
    const events = [];
    for (const t of ['transitionrun', 'transitionstart', 'transitionend', 'transitioncancel']) {
      el.addEventListener(t, (e) => events.push(e.type.slice(10) + ':' + e.propertyName));
    }
    el.classList.add('on');
    getComputedStyle(el).getPropertyValue(read);
    const anims = document.getAnimations();
    const at = {};
    for (const t of [0, 1, 50, 99]) {
      anims.forEach((a) => { a.pause(); a.currentTime = t; });
      at[t] = getComputedStyle(el).getPropertyValue(read);
    }
    anims.forEach((a) => { a.currentTime = 50; });
    const mid = getComputedStyle(el).getPropertyValue(read);
    const names = anims.map((a) => a.transitionProperty);
    anims.forEach((a) => a.finish());
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
    return { mid, at, transitions: names, events, end: getComputedStyle(el).getPropertyValue(read) };
  }, read);
  console.log(`${label}: ${JSON.stringify(out.at)} | anims [${out.transitions}] | events [${out.events}] | end ${out.end}`);
  cases.push({ label, css, read, ...out });
}

// visibility: discrete, but visible wins whenever one end is visible
await probe('visible->hidden', '#h{visibility:visible}#h.on{visibility:hidden}', 'visibility');
await probe('hidden->visible', '#h{visibility:hidden}#h.on{visibility:visible}', 'visibility');
await probe('visible->collapse', '#h{visibility:visible}#h.on{visibility:collapse}', 'visibility');
await probe('hidden->collapse', '#h{visibility:hidden}#h.on{visibility:collapse}', 'visibility');
await probe('named visibility only', '#h{visibility:visible;transition:visibility 100ms linear}#h.on{visibility:hidden}', 'visibility');
await probe('visible->hidden with opacity', '#h{visibility:visible;opacity:1}#h.on{visibility:hidden;opacity:0}', 'visibility');

await browser.close();
writeFileSync(RECEIPT, JSON.stringify({ chrome: PIN, sampled_at_ms: 50, duration_ms: 100, timing: 'linear', cases }, null, 2) + '\n');
console.log('receipt ' + RECEIPT);
