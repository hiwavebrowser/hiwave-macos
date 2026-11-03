// node --test docs/census/tools/css_property_names.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { authoredProperties, isBlockedTitle, neverLoad, publishable } from './css_property_names.mjs';

const json = (rel) => JSON.parse(readFileSync(new URL(rel, import.meta.url), 'utf8'));
const known = json('./css_known_properties.json').properties;

const names = (css) => [...authoredProperties(css)].sort();

test('declarations are read, in any case and spacing', () => {
  assert.deepEqual(names('a{COLOR : red;margin-top:1px}b{-webkit-mask-image:none}'), ['-webkit-mask-image', 'color', 'margin-top']);
});

test('selectors and media queries are not declarations', () => {
  assert.deepEqual(names('a:hover{top:0}@media (max-width:1px){p{left:0}}'), ['left', 'top']);
});

test('a data url, a string or a comment adds nothing', () => {
  assert.deepEqual(names('a{background:url(data:image/svg+xml;fill:red;x:y);content:"{right:0;";/* ;bottom:0; */top:0}'), ['background', 'content', 'top']);
});

test('custom properties are not counted', () => {
  assert.deepEqual(names('a{--x:1;color:var(--x)}'), ['color']);
});

test('a style attribute opened with a brace reads whole', () => {
  assert.deepEqual(names('{top:0;left:1px'), ['left', 'top']);
});

test('ebay is never loaded, on any of its hosts', () => {
  assert.equal(neverLoad('https://www.ebay.com/'), true);
  assert.equal(neverLoad('https://ebay.co.uk/'), true);
  assert.equal(neverLoad('https://www.notebay.com/'), false);
  assert.equal(neverLoad('not a url'), true);
});

test('a challenge page is not a loaded site', () => {
  assert.equal(isBlockedTitle('Just a moment...'), true);
  assert.equal(isBlockedTitle('News'), false);
});

test('an @font-face descriptor is not a property, a real declaration still is', () => {
  assert.deepEqual(names('@font-face{font-family:F;src:url(a.woff);font-stretch:75% 100%;font-style:italic}a{top:0}'), ['top']);
  assert.deepEqual(names('@FONT-FACE {font-stretch:75%}@media print{@font-face{font-style:italic}p{font-weight:700}}'), ['font-weight']);
  assert.deepEqual(names('@font-face{font-stretch:75%}a{font-stretch:condensed}'), ['font-stretch']);
});

test('shopify is never loaded either, nor the hosts that serve for these two', () => {
  assert.equal(neverLoad('https://www.shopify.com/'), true);
  assert.equal(neverLoad('https://cdn.shopify.com/a.css'), true);
  assert.equal(neverLoad('https://shop.myshopify.com/'), true);
  assert.equal(neverLoad('https://x.shopifycdn.com/a.css'), true);
  assert.equal(neverLoad('https://i.ebayimg.com/a.png'), true);
  assert.equal(neverLoad('https://ir.ebaystatic.com/a.css'), true);
  assert.equal(neverLoad('https://www.notshopify.com/'), false);
});

test('only names Chromium knows are published', () => {
  const counts = { color: 3, 'x-custom-card': 1, xbuttoncolor: 2, src: 9, '-moz-appearance': 4, '-webkit-box-orient': 5 };
  assert.deepEqual(publishable(counts, ['color', '-webkit-box-orient']), { '-webkit-box-orient': 5, color: 3 });
});

// The file is public: a name read from a nested selector or a site's private
// declaration can say whose page it came from.
test('the checked-in prevalence file holds only names Chromium knows', () => {
  const file = json('../css_property_prevalence_top100.json');
  const allowed = new Set(known);
  assert.deepEqual(Object.keys(file.sites_authoring).filter((name) => !allowed.has(name)), []);
});

test('the prevalence file\'s meta holds no URL and only the fields it should', () => {
  const { meta } = json('../css_property_prevalence_top100.json');
  assert.deepEqual(Object.keys(meta).sort(), ['asked', 'chromium', 'date', 'loaded', 'measure', 'universe']);
  assert.doesNotMatch(JSON.stringify(meta), /https?:/);
});

test('the known list is property-shaped and holds every census row', () => {
  for (const name of known) assert.match(name, /^(-webkit-)?[a-z]+(-[a-z0-9]+)*$/);
  const allowed = new Set(known);
  const rows = json('../../../trench/devtools/tools/css_census/properties.json');
  assert.deepEqual(rows.map((r) => r.name).filter((name) => !allowed.has(name)), []);
});
