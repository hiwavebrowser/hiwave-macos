// prevalence_visit.mjs - one site's visit for css_property_prevalence.mjs, split out so the drop
// rules can be tested against a local server (never_load.test.mjs maps every host to 127.0.0.1).
import { authoredProperties, isBlockedTitle } from './css_property_names.mjs';
import { watchRefused } from './never_load.mjs';

// Text a stylesheet's author wrote: <style> text, rules a script inserted into an empty <style>, and
// style attributes, each attribute opened with `{` so its first declaration reads like one.
export const PAGE_CSS = `(() => {
  const out = [];
  for (const el of document.querySelectorAll('style')) {
    if (el.textContent.trim()) { out.push(el.textContent); continue; }
    try { for (const r of el.sheet.cssRules) out.push(r.cssText); } catch {}
  }
  for (const el of document.querySelectorAll('[style]')) out.push('{' + el.getAttribute('style'));
  return out;
})()`;

// One visit. `outcome` is 'loaded' (names set), 'dropped' (the load asked for a refused host) or
// 'failed' (no page to read). `refused` counts requests the handler aborted.
export async function visitSite(browser, url, { viewport, settleMs = 1500 } = {}) {
  const ctx = await browser.newContext({ viewport, deviceScaleFactor: 1, locale: 'en-US', timezoneId: 'America/New_York' });
  const watch = await watchRefused(ctx);
  const page = await ctx.newPage();
  const bodies = [];
  page.on('response', (resp) => {
    const req = resp.request();
    if (req.resourceType() !== 'stylesheet' || req.frame() !== page.mainFrame()) return;
    bodies.push(resp.text().catch(() => ''));
  });
  let outcome = 'failed';
  let names = null;
  try {
    const resp = await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
    await page.waitForLoadState('networkidle', { timeout: 8000 }).catch(() => {});
    await page.waitForTimeout(settleMs);
    const status = resp ? resp.status() : 0;
    if ([401, 403, 429, 503].includes(status) || status >= 500 || isBlockedTitle(await page.title())) {
      return { outcome, names, refused: watch.refused };
    }
    const css = await page.evaluate(PAGE_CSS);
    await Promise.all(bodies);
    if (watch.touched) {
      outcome = 'dropped';
      return { outcome, names, refused: watch.refused };
    }
    names = new Set();
    for (const text of [...css, ...(await Promise.all(bodies))]) {
      for (const name of authoredProperties(text)) names.add(name);
    }
    outcome = 'loaded';
    return { outcome, names, refused: watch.refused };
  } catch {
    return { outcome: 'failed', names: null, refused: watch.refused };
  } finally {
    await ctx.close().catch(() => {});
  }
}
