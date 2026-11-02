// node --test docs/census/tools/css_property_names.test.mjs
import test from 'node:test';
import assert from 'node:assert/strict';
import { authoredProperties, isBlockedTitle, neverLoad } from './css_property_names.mjs';

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
