#!/usr/bin/env node
/**
 * keyword_grammar_log.mjs - which single keywords Chromium accepts for a
 * keyword-only property (Z plan CH2.E29b).
 *
 *   PARITY_CHROME_PATH=<pinned Chrome for Testing> node keyword_grammar_log.mjs <props.json>
 *
 * props.json: { "properties": [...], "extra_keywords": [...] }. The pool is
 * every identifier Chromium's own computed style uses for any property, plus
 * `extra_keywords`. For each property it prints the pool keywords
 * `CSS.supports` accepts, and a list of non-keyword forms (numbers, lengths,
 * strings, two keywords) it accepts too: a property with any of those is not
 * keyword-only and must not go in the bindings table. Headless always.
 */
import { chromium } from 'playwright';
import { readFileSync } from 'fs';

const doc = JSON.parse(readFileSync(process.argv[2], 'utf8'));
const browser = await chromium.launch({ headless: true, executablePath: process.env.PARITY_CHROME_PATH || undefined });
const page = await browser.newPage();
await page.setContent('<!doctype html><body><div id=a>x</div><span>y</span><img><input><table><tr><td>z</td></tr></table>');
const out = await page.evaluate((doc) => {
  const pool = new Set(doc.extra_keywords.map((k) => k.toLowerCase()));
  for (const el of document.querySelectorAll('*')) {
    const s = getComputedStyle(el);
    for (let i = 0; i < s.length; i++) {
      for (const w of s.getPropertyValue(s[i]).split(/[\s,()/]+/)) {
        if (/^-?[a-z][a-z0-9-]*$/i.test(w)) pool.add(w.toLowerCase());
      }
    }
  }
  const forms = ['0', '1', '1px', '50%', '"s"', 'a b', 'auto auto', 'none none', 'calc(1px)', '1deg'];
  const res = { pool: pool.size, properties: {} };
  for (const p of doc.properties) {
    const kw = [...pool].filter((k) => !['initial', 'inherit', 'unset', 'revert', 'revert-layer'].includes(k) && CSS.supports(p, k)).sort();
    const other = forms.filter((f) => CSS.supports(p, f));
    res.properties[p] = { keywords: kw, other };
  }
  return res;
}, doc);
console.log(JSON.stringify({ chromium: browser.version(), ...out }, null, 1));
await browser.close();
