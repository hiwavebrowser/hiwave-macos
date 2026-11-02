// css_property_names.mjs - the pure parts of css_property_prevalence.mjs, so they can be tested with no browser.

// Declarations only: a name right after `{` or `;`. A selector's `a:hover` follows neither unless it
// opens a nested block, and `@media (max-width:1px)` follows `(`. Comments, strings and url() go first:
// a data: URL carries `;` and `:` of its own. Custom properties are not properties here.
export function authoredProperties(css) {
  const text = css
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/url\(\s*(?:"[^"]*"|'[^']*'|[^)]*)\)/gi, 'url()')
    .replace(/"[^"]*"|'[^']*'/g, '""');
  const names = new Set();
  for (const m of text.matchAll(/[{;]\s*(-?[a-zA-Z][a-zA-Z-]*)\s*:/g)) {
    names.add(m[1].toLowerCase());
  }
  return names;
}

export function neverLoad(url) {
  try {
    return /(^|\.)ebay\./i.test(new URL(url).hostname);
  } catch {
    return true;
  }
}

export function isBlockedTitle(title) {
  return /just a moment|access denied|attention required|captcha|challenge|bot or not|security checkpoint|are you a human|verify you are/i.test(title || '');
}
