// JETSTREAM-IFRAME-SYNC receipt: an iframe's contentDocument right after insertion, and
// document.open / write / close into it, in the pinned Chrome for Testing.
// Run: PARITY_CHROME_PATH=<CfT 148> node tools/parity_oracle/iframe_write_log.mjs
// Output: docs/diagnostics/2026-11-23/chrome148_iframe_write.json
import { chromium } from 'playwright';
import { getDeterministicLaunchOptions } from './deterministic.mjs';
const browser = await chromium.launch(getDeterministicLaunchOptions({ requirePinned: true }));
try {
  const p = await browser.newPage();
  await p.setContent('<!doctype html><body></body>');
  const r = await p.evaluate(() => {
    const out = {};
    const a = document.createElement('iframe');
    out.before_insert = a.contentDocument === null;
    document.body.appendChild(a);
    const d = a.contentDocument;
    out.no_src = d ? { url: d.URL, ready: d.readyState, html: d.documentElement && d.documentElement.outerHTML, body: !!d.body, win: !!a.contentWindow, same: a.contentWindow.document === d } : null;
    const b = document.createElement('iframe'); b.src = 'about:blank'; document.body.appendChild(b);
    out.src_blank = b.contentDocument ? { url: b.contentDocument.URL, body: !!b.contentDocument.body } : null;
    // JetStream's pattern
    a.contentDocument.body.innerHTML = '<iframe id="magicframe" frameborder="0">';
    const m = a.contentDocument.getElementById('magicframe');
    out.nested = m && m.contentDocument ? { url: m.contentDocument.URL, body: !!m.contentDocument.body } : null;
    window.__log = [];
    const md = m.contentDocument;
    md.open();
    out.after_open = { body: md.body, html: md.documentElement && md.documentElement.outerHTML };
    md.write('<!DOCTYPE html><html><head><script>top.__log.push("s1:" + document.readyState);</script></head><body><p id="x">hi</p><script>top.__log.push("s2:" + !!document.getElementById("x"));</script>');
    out.after_write = { log: window.__log.slice(), ready: md.readyState, p: !!md.getElementById('x') };
    md.write('<script>top.__log.push("s3");</script>');
    md.close();
    out.after_close = { log: window.__log.slice(), ready: md.readyState, html_len: md.documentElement.outerHTML.length };
    return out;
  });
  console.log(JSON.stringify(r, null, 1));
} finally { await browser.close(); }
