// css_property_names.mjs - the pure parts of css_property_prevalence.mjs, so they can be tested with no browser.

// Declarations only: a name right after `{` or `;`. A selector's `a:hover` follows neither unless it
// opens a nested block, and `@media (max-width:1px)` follows `(`. Comments, strings and url() go first:
// a data: URL carries `;` and `:` of its own. Custom properties are not properties here, nor are
// @font-face descriptors: `font-stretch` in one says what a font covers, not how an element renders.
export function authoredProperties(css) {
  const text = css
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/url\(\s*(?:"[^"]*"|'[^']*'|[^)]*)\)/gi, 'url()')
    .replace(/"[^"]*"|'[^']*'/g, '""')
    .replace(/@font-face\s*\{[^}]*\}/gi, ' ');
  const names = new Set();
  for (const m of text.matchAll(/[{;]\s*(-?[a-zA-Z][a-zA-Z-]*)\s*:/g)) {
    names.add(m[1].toLowerCase());
  }
  return names;
}

// Kept here for the tests that import it; the list lives in never_load.mjs.
export { neverLoad } from './never_load.mjs';

// The public file: names Chromium knows, sorted. Anything else the reader saw (a custom element from a
// nested selector, a site's private declaration, a typo) can say whose page it came from.
export function publishable(counts, known) {
  const allowed = new Set(known);
  const names = Object.keys(counts).filter((name) => allowed.has(name)).sort();
  return Object.fromEntries(names.map((name) => [name, counts[name]]));
}

export function isBlockedTitle(title) {
  return /just a moment|access denied|attention required|captcha|challenge|bot or not|security checkpoint|are you a human|verify you are/i.test(title || '');
}

