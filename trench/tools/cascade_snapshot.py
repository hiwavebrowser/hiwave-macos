#!/usr/bin/env python3
"""Pin a page for the cascade-speed trench: its HTML plus every stylesheet.

    python3 trench/tools/cascade_snapshot.py            # cnn, github, wikipedia
    python3 trench/tools/cascade_snapshot.py wikipedia  # one site

Writes trench/cascade/snapshots/<site>/index.html and css/NN.css.
- <link rel=stylesheet> hrefs are downloaded and rewritten to css/NN.css, so
  the bench loads them as external sheets (parsed once, as on a live load).
- url(...) and @import inside each sheet are made absolute against the
  sheet's original URL (fonts/images still resolve to the real origin).
- <script> elements are removed. RustKit's page JS runs against a stub
  document (realsite PLAN 2026-09-27 20:05), so scripts cannot change the
  DOM the cascade sees; removing them makes the load deterministic.
- manifest.json records the source URLs, byte sizes and fetch time.

Re-pinning changes the measured workload: that needs a dated line under
Changes in trench/BASELINE-cascade.md.
"""
import html
import json
import os
import re
import sys
import time
import urllib.request
from urllib.parse import urljoin

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "cascade", "snapshots")
SITES = {
    "cnn": "https://www.cnn.com/",
    "github": "https://github.com/",
    "wikipedia": "https://en.wikipedia.org/wiki/Web_browser",
}
UA = ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36")

LINK = re.compile(r"<link\b[^>]*>", re.I)
REL_SHEET = re.compile(r"""\brel\s*=\s*["']?[^"'>]*\bstylesheet\b""", re.I)
HREF = re.compile(r"""\bhref\s*=\s*(["'])(.*?)\1|\bhref\s*=\s*([^\s>]+)""", re.I | re.S)
SCRIPT = re.compile(r"<script\b[^>]*>.*?</script\s*>", re.I | re.S)
BASE = re.compile(r"""<base\b[^>]*\bhref\s*=\s*["']([^"']+)""", re.I)
CSS_URL = re.compile(r"""url\(\s*(["']?)([^"')]+)\1\s*\)""", re.I)
CSS_IMPORT = re.compile(r"""@import\s+(["'])([^"']+)\1""", re.I)


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept-Language": "en-US,en;q=0.9"})
    with urllib.request.urlopen(req, timeout=30) as r:
        return r.geturl(), r.read()


def absolutize_css(css, base):
    def fix(m):
        ref = m.group(2).strip()
        if ref.startswith(("data:", "#")):
            return m.group(0)
        return "url(%s%s%s)" % (m.group(1), urljoin(base, ref), m.group(1))
    css = CSS_URL.sub(fix, css)
    return CSS_IMPORT.sub(lambda m: "@import %s%s%s" % (m.group(1), urljoin(base, m.group(2)), m.group(1)), css)


def snapshot(site, url):
    final_url, body = fetch(url)
    doc = body.decode("utf-8", errors="replace")
    base_m = BASE.search(doc)
    base = urljoin(final_url, html.unescape(base_m.group(1))) if base_m else final_url
    out = os.path.join(OUT, site)
    os.makedirs(os.path.join(out, "css"), exist_ok=True)
    sheets = []

    def rewrite(m):
        tag = m.group(0)
        if not REL_SHEET.search(tag):
            return tag
        h = HREF.search(tag)
        if not h:
            return tag
        href = html.unescape(h.group(2) if h.group(2) is not None else h.group(3))
        src = urljoin(base, href)
        # Pages link some sheets more than once (github: 4x primer-react-brand).
        # Every <link> stays, as the page has it; the file is stored once.
        seen = next((s for s in sheets if s["src"] == src and "file" in s), None)
        if seen:
            sheets.append(dict(seen))
            return tag[:h.start()] + 'href="%s"' % seen["file"] + tag[h.end():]
        name = "css/%02d.css" % len(sheets)
        try:
            sheet_url, data = fetch(src)
        except Exception as e:  # a sheet the page can't load stays out, as in a browser
            sheets.append({"src": src, "error": str(e)})
            return ""
        css = absolutize_css(data.decode("utf-8", errors="replace"), sheet_url)
        with open(os.path.join(out, name), "w") as f:
            f.write(css)
        sheets.append({"src": src, "file": name, "bytes": len(css.encode())})
        return tag[:h.start()] + 'href="%s"' % name + tag[h.end():]

    doc = LINK.sub(rewrite, doc)
    n_scripts = len(SCRIPT.findall(doc))
    doc = SCRIPT.sub("", doc)
    # A <base> would send the local css/NN.css hrefs back to the origin.
    doc = re.sub(r"<base\b[^>]*>", "", doc, flags=re.I)
    with open(os.path.join(out, "index.html"), "w") as f:
        f.write(doc)
    manifest = {
        "site": site, "url": url, "final_url": final_url,
        "fetched": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "html_bytes": len(doc.encode()), "scripts_removed": n_scripts, "sheets": sheets,
    }
    with open(os.path.join(out, "manifest.json"), "w") as f:
        json.dump(manifest, f, indent=1)
    ok = sum(1 for s in sheets if "file" in s)
    print("%-10s html %7d B  sheets %d/%d  css %8d B  scripts removed %d" % (
        site, manifest["html_bytes"], ok, len(sheets),
        sum(s.get("bytes", 0) for s in sheets), n_scripts))


if __name__ == "__main__":
    for site in sys.argv[1:] or SITES:
        snapshot(site, SITES[site])
