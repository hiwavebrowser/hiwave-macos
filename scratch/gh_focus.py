"""Find rules in github.com's stylesheets whose box-shadow uses the focus colour."""
import re, sys, urllib.request

ua = {'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15'}
pat = sys.argv[1] if len(sys.argv) > 1 else r'box-shadow:\s*inset 0 0 0 2px var\(--focus'
html = urllib.request.urlopen(urllib.request.Request('https://github.com/', headers=ua), timeout=20).read().decode('utf8', 'replace')
links = re.findall(r'<link[^>]+href="([^"]+\.css)"', html)
print(len(links), "stylesheets")
hits = []
for l in links:
    css = urllib.request.urlopen(urllib.request.Request(l, headers=ua), timeout=20).read().decode('utf8', 'replace')
    for m in re.finditer(r'([^{}]{1,400})\{([^{}]*' + pat + r'[^{}]*)\}', css):
        hits.append((l.rsplit('/', 1)[-1], m.group(1).strip()[-200:], m.group(2)[:160]))
for h in hits[:30]:
    print(h)
print(len(hits), "hits")
