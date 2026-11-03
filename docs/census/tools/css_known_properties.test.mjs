// node --test docs/census/tools/css_known_properties.test.mjs
// The public prevalence file may hold only names in css_known_properties.json, and that list is the
// guard's trust root: a name added to both files would pass css_property_names.test.mjs. So the list
// itself is checked against the Chromium the crawl uses (tools/parity_oracle's Playwright), read on
// a blank page with no network.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { KNOWN_PROPERTIES } from './css_property_names.mjs';

const require = createRequire(new URL('../../../tools/parity_oracle/package.json', import.meta.url));
const { chromium } = require('playwright');

const committed = JSON.parse(readFileSync(new URL('./css_known_properties.json', import.meta.url), 'utf8'));

let chromiumKnows;
let version;
test.before(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    version = browser.version();
    const page = await browser.newPage();
    chromiumKnows = new Set(await page.evaluate(KNOWN_PROPERTIES));
  } finally {
    await browser.close();
  }
});

test('every name in the known list is a property Chromium knows', () => {
  const extra = committed.properties.filter((name) => !chromiumKnows.has(name));
  assert.deepEqual(extra, [], `names Chromium ${version} does not know (the list was read from ${committed.chromium})`);
});

test('the known list has no duplicates and is sorted, as the crawl writes it', () => {
  assert.deepEqual(committed.properties, [...new Set(committed.properties)].sort());
});

test('the Chromium read is not empty, so the check above can fail', () => {
  assert.ok(chromiumKnows.size > 300, `only ${chromiumKnows.size} names read`);
  assert.ok(chromiumKnows.has('color') && !chromiumKnows.has('src'), 'a property is known and a descriptor is not');
});
