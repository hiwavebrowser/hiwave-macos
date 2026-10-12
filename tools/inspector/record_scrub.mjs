/**
 * record_scrub.mjs - what of a URL a record may carry.
 *
 * Shared by chrome_records.mjs (applied in Node, after the page returns its
 * raw values) and its tests. No Playwright import, so it loads anywhere.
 *
 * A URL keeps scheme, host, port and path: never a user name or password,
 * a query or a fragment, and a data: URL is `data:…`. A file: URL is the
 * path relative to the page's directory (a host path never reaches the
 * record); a file: resource under a non-file page is `file:…`.
 */

import { posix } from 'path';

export function scrubUrl(u, pageUrl) {
  const raw = String(u).trim();
  if (/^data:/i.test(raw)) return 'data:…';
  let url;
  try {
    url = new URL(raw);
  } catch {
    // Relative, as specified: no authority unless scheme-relative.
    const bare = raw.split(/[?#]/)[0];
    return bare.startsWith('//') ? bare.replace(/^\/\/[^/]*@/, '//') : bare;
  }
  if (url.protocol === 'file:') {
    let page;
    try { page = new URL(pageUrl); } catch { page = null; }
    if (!page || page.protocol !== 'file:') return 'file:…';
    const from = posix.dirname(decodeURIComponent(page.pathname));
    return posix.relative(from, decodeURIComponent(url.pathname)) || posix.basename(url.pathname);
  }
  url.username = '';
  url.password = '';
  url.search = '';
  url.hash = '';
  return url.toString();
}

const GRADIENT = /^(?:-webkit-)?((?:repeating-)?(?:linear|radial|conic)-gradient)\(/i;
const URL_TOKEN = /url\(\s*(["']?)(.*?)\1\s*\)/gi;

/** getComputedStyle's background-image, every url() token scrubbed and
 * gradients named but not spelled out (as the engine's record does). */
export function backgroundImage(v, pageUrl) {
  if (!v || v === 'none') return 'none';
  const layers = [];
  let depth = 0;
  let cur = '';
  let quote = null;
  for (const ch of v) {
    if (quote) { if (ch === quote) quote = null; cur += ch; continue; }
    if (ch === '"' || ch === "'") quote = ch;
    if (ch === '(') depth++;
    if (ch === ')') depth--;
    if (ch === ',' && depth === 0) { layers.push(cur.trim()); cur = ''; continue; }
    cur += ch;
  }
  layers.push(cur.trim());
  return layers.map((l) => {
    const g = l.match(GRADIENT);
    if (g) return `${g[1].toLowerCase()}(…)`;
    return l.replace(URL_TOKEN, (_, _q, u) => `url("${scrubUrl(u, pageUrl)}")`);
  }).join(', ');
}

/** The page as a record names it: a URL scrubbed (query kept on request),
 * a local file by the path as given when relative, else relative to the
 * working directory, else its file name. */
export function reportTarget(target, isUrl, keepQuery, cwd) {
  if (isUrl) {
    if (keepQuery) {
      try { const u = new URL(target); u.username = ''; u.password = ''; return u.toString(); } catch { return target; }
    }
    return /^(data|about):/i.test(target) ? target.split(/[,?#]/)[0] : scrubUrl(target, null);
  }
  if (!posix.isAbsolute(target)) return target;
  const rel = posix.relative(cwd, target);
  return rel && !rel.startsWith('..') ? rel : posix.basename(target);
}
