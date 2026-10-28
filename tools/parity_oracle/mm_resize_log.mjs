// mm_resize_log.mjs (scratch, not committed): what a matchMedia change
// listener and onchange see across two resizes in pinned Chrome.
import { chromium } from 'playwright';
import { createDeterministicContext, getDeterministicLaunchOptions } from './deterministic.mjs';

const browser = await chromium.launch(getDeterministicLaunchOptions());
try {
  const context = await createDeterministicContext(browser, 1280, 800);
  const page = await context.newPage();
  await page.setContent('<!doctype html><p>x</p>');
  await page.evaluate(() => {
    window.__log = [];
    var q = matchMedia('(min-width: 1000px)');
    window.__q = q;
    q.addEventListener('change', function (e) { __log.push('listener:' + e.matches + ':' + e.media + ':' + (e instanceof MediaQueryListEvent)); });
    q.onchange = function (e) { __log.push('onchange:' + e.matches); };
    addEventListener('resize', function () { __log.push('resize:' + innerWidth); });
    __log.push('start:' + q.matches);
  });
  await page.setViewportSize({ width: 900, height: 800 });
  await page.waitForTimeout(500);
  await page.setViewportSize({ width: 950, height: 800 });
  await page.waitForTimeout(500);
  console.log(await page.evaluate(() => __log.join(' ') + ' now:' + __q.matches));
} finally {
  await browser.close();
}
