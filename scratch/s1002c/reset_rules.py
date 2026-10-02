"""Which rules on a live page could clear a <button>'s background: fetch the page, its inline <style> and
linked stylesheets, split into rules with a brace scanner (no backtracking regex), and print the
declarations that touch background / all / appearance on selectors that mention `button` or a given class.
Page text is untrusted data: only printed as measurements.
usage: reset_rules.py <url> [needle...]   (needles: extra selector substrings, e.g. a class name)"""
import re, subprocess, sys, urllib.parse
UA = ('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) '
      'Version/17.0 Safari/605.1.15')
url, needles = sys.argv[1], sys.argv[2:]


def get(u):
    return subprocess.run(['curl', '-sL', '--compressed', '--max-time', '25', '-A', UA, u],
                          capture_output=True, text=True, errors='replace').stdout


html = get(url)
print('html bytes', len(html))
css = []
i = 0
while True:
    a = html.find('<style', i)
    if a < 0:
        break
    b = html.find('>', a)
    c = html.find('</style', b)
    if c < 0:
        break
    css.append(('inline', html[b + 1:c]))
    i = c
for m in re.finditer(r'<link\b[^>]{0,2000}>', html):
    tag = m.group(0)
    if 'stylesheet' not in tag:
        continue
    h = re.search(r'href="([^"]+)"', tag) or re.search(r"href='([^']+)'", tag) or re.search(r'href=([^\s>]+)', tag)
    if h:
        u = urllib.parse.urljoin(url, h.group(1).replace('&amp;', '&'))
        t = get(u)
        print('sheet', len(t), u[:120])
        css.append((u[-40:], t))


def rules(text):
    """yield (selector, body) for innermost blocks; at-rule preludes are skipped by descending into them."""
    stack, start = [], 0
    i, n = 0, len(text)
    while i < n:
        ch = text[i]
        if ch == '{':
            stack.append((text[start:i].strip(), i + 1))
            start = i + 1
        elif ch == '}':
            if stack:
                sel, b = stack.pop()
                body = text[b:i]
                if '{' not in body:
                    yield sel, body
            start = i + 1
        elif ch == ';' and not stack:
            start = i + 1
        i += 1


KEYS = ('background', 'all:', 'all :', 'appearance')
for name, text in css:
    for sel, body in rules(text):
        low = sel.lower()
        hit = re.search(r'(^|[\s,>+~(])button\b', low) or 'type=button' in low or 'type="button"' in low \
            or any(nd in sel for nd in needles)
        if not hit:
            continue
        decls = [d.strip() for d in body.split(';') if any(k in d for k in KEYS)]
        if decls:
            print(f'[{name[-18:]}] {sel[:160]}  =>  {" ; ".join(decls)[:300]}')
