/**
 * record_scrub.mjs - what of a URL a record may carry.
 *
 * Shared by chrome_records.mjs (applied in Node, after the page returns its
 * raw values) and its tests. No Playwright import, so it loads anywhere.
 *
 * A URL keeps scheme, host, port and path: never a user name or password,
 * a query, a fragment or a `;` path parameter (`;jsessionid=` is a session
 * cookie), and a data: URL is `data:…`, a javascript: URL `javascript:` and
 * a blob: URL `blob:` and its origin. A file: URL is the path relative to
 * the page's directory (a host path never reaches the record); a file:
 * resource under a non-file page is `file:…`. These are the engine's rules
 * (report_url and scrub_url in crates/rustkit-engine/src/diagnostics.rs),
 * applied to the URL parser's view of the string.
 */

import { posix } from 'path';

/** The string as the URL parser reads it: ASCII tab and newline dropped
 * anywhere, leading and trailing C0 controls and spaces trimmed (and any
 * leading Unicode space, to record less). */
function parserView(u) {
  return String(u).replace(/[\t\n\r]/g, '').replace(/^[\x00-\x20\s]+/u, '').replace(/[\x00-\x20]+$/, '');
}

/** A path without its `;` parameters (`/cart;jsessionid=S/x` is `/cart/x`):
 * each runs from `;` (or `%3B`, to record less) to the end of its segment.
 * The query and fragment must already be cut. `backslash`: `\` also ends a
 * segment, as it does in a relative URL. */
export function dropPathParams(path, backslash) {
  const end = backslash ? /[/\\]/ : /\//;
  let out = '';
  let rest = path;
  for (;;) {
    const at = rest.search(/;|%3b/i);
    if (at < 0) return out + rest;
    out += rest.slice(0, at);
    const e = rest.slice(at).search(end);
    rest = e < 0 ? '' : rest.slice(at + e);
  }
}

/** The page URL as a record reports it (the engine's report_url): no user
 * name or password, and no query, fragment or `;` path parameters unless
 * `keepQuery`. data:, javascript: and blob: are reduced either way. */
export function reportUrl(url, keepQuery) {
  if (url.protocol === 'data:') {
    // With no comma there is no media type, only payload.
    const comma = url.pathname.indexOf(',');
    return `data:${comma < 0 ? '' : url.pathname.slice(0, comma).split(';')[0].trim()}`;
  }
  if (url.protocol === 'javascript:') return 'javascript:';
  if (url.protocol === 'blob:') return url.origin === 'null' ? 'blob:' : `blob:${url.origin}`;
  const u = new URL(url.href);
  u.username = '';
  u.password = '';
  if (keepQuery) return u.href;
  u.search = '';
  u.hash = '';
  const path = dropPathParams(u.pathname, false);
  // An opaque path (`mailto:a;b`) cannot be set: spell it out.
  if (!u.pathname.startsWith('/')) return `${u.protocol}${path}`;
  u.pathname = path;
  return u.href;
}

export function scrubUrl(u, pageUrl) {
  const raw = parserView(u);
  let url;
  try { url = new URL(raw); } catch { url = null; }
  if (/^data:/i.test(raw) || url?.protocol === 'data:') return 'data:…';
  if (!url) {
    // Relative, as specified (or unparsable): userinfo only in an authority,
    // and `\` ends a segment as it does once resolved against an http page.
    const bare = raw.split(/[?#]/)[0];
    const m = bare.match(/^((?:[a-z][a-z0-9+.-]*:)?[/\\]{2})([^/\\]*)(.*)$/is);
    if (!m) return dropPathParams(bare, true);
    return m[1] + m[2].slice(m[2].lastIndexOf('@') + 1) + dropPathParams(m[3], true);
  }
  if (url.protocol === 'file:') {
    let page;
    try { page = new URL(pageUrl); } catch { page = null; }
    if (!page || page.protocol !== 'file:') return 'file:…';
    const from = posix.dirname(decodeURIComponent(page.pathname));
    const path = dropPathParams(url.pathname, false);
    return posix.relative(from, decodeURIComponent(path)) || posix.basename(path);
  }
  return reportUrl(url, false);
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

/** Whether chrome_records.mjs takes `target` as a URL (else an html file):
 * decided on the parser's view, so a URL spelled with `\\` or a tab is one. */
export function isUrlTarget(target) {
  return /^(?:[a-z][a-z0-9+.-]*:[/\\]{2}|(?:data|about|blob|javascript|https?|wss?|ftp):)/i.test(parserView(target));
}

/** The page as a record names it: a URL scrubbed (query and path
 * parameters kept on request),
 * a local file by the path as given when relative, else relative to the
 * working directory, else its file name. */
export function reportTarget(target, isUrl, keepQuery, cwd) {
  if (isUrl) {
    let u;
    try { u = new URL(parserView(target)); } catch { return scrubUrl(target, null); }
    return reportUrl(u, keepQuery);
  }
  if (!posix.isAbsolute(target)) return target;
  const rel = posix.relative(cwd, target);
  return rel && !rel.startsWith('..') ? rel : posix.basename(target);
}

/** A message (a navigation error, an exception) as a result may carry it:
 * each target, as given or as a URL spells it, becomes the record's name
 * for it, and any other URL left in it is scrubbed. */
export function redactTarget(msg, targets, named) {
  let s = String(msg);
  const forms = new Set();
  for (const t of targets) {
    if (!t) continue;
    forms.add(String(t));
    try { forms.add(new URL(parserView(t)).href); } catch { /* not a URL: the form as given */ }
  }
  for (const f of [...forms].sort((a, b) => b.length - a.length)) s = s.split(f).join(named);
  // A data: payload runs to the end of the line, spaces and all.
  s = s.replace(/data:([^,\s]*),[^\n]*/gi, (_, m) => `data:${m.split(';')[0]}`);
  // So does a script URL's; a blob URL keeps its origin.
  s = s.replace(/\bjavascript:[^\n]*/gi, 'javascript:');
  s = s.replace(/\bblob:(?![a-z][a-z0-9+.-]*:\/\/)[^\s"'<>]*/gi, 'blob:');
  return s.replace(/\b(?:blob:)?[a-z][a-z0-9+.-]*:\/\/[^\s"'<>]*/gi, (u) => (u === named ? u : scrubUrl(u, null)));
}
