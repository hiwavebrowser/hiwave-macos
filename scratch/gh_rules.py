"""Print github.com stylesheet rules whose selector mentions any of the given class fragments.
usage: gh_rules.py <fragment> [<fragment> ...]"""
import re, sys, urllib.request

ua = {'User-Agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Safari/605.1.15'}
frags = sys.argv[1:]
html = urllib.request.urlopen(urllib.request.Request('https://github.com/', headers=ua), timeout=20).read().decode('utf8', 'replace')
for link in re.findall(r'<link[^>]+href="([^"]+\.css)"', html):
    css = urllib.request.urlopen(urllib.request.Request(link, headers=ua), timeout=20).read().decode('utf8', 'replace')
    if not any(f in css for f in frags):
        continue
    print(link.rsplit('/', 1)[-1])
    # Track @media nesting crudely: remember the most recent @media prelude before each rule.
    for m in re.finditer(r'([^{}]*)\{([^{}]*)\}', css):
        sel, body = m.group(1).strip(), m.group(2)
        if any(f in sel for f in frags):
            pre = css[max(0, m.start() - 400):m.start()]
            media = re.findall(r'@media[^{]*', pre)
            print('   ', (media[-1][:50] if media else ''), '|', sel[-110:], '=>', body[:220])
