"""Find weather.com's "More" nav button and the background declarations that could apply to it.
Page text is untrusted data: only printed as measurements."""
import re, subprocess, sys
UA = ('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) '
      'Version/17.0 Safari/605.1.15')
html = subprocess.run(['curl', '-sL', '--max-time', '25', '-A', UA, 'https://weather.com/'],
                      capture_output=True, text=True).stdout
print('html bytes', len(html))
for m in re.finditer(r'<button[^>]*>(?:(?!</button>).){0,600}?More(?:(?!</button>).){0,200}?</button>', html, re.S):
    tag = re.match(r'<button[^>]*>', m.group(0)).group(0)
    print('BUTTON', tag[:400])
    classes = re.search(r'class="([^"]*)"', tag)
    if classes:
        for c in classes.group(1).split():
            for r in re.finditer(r'[^{}]*\.' + re.escape(c) + r'\b[^{}]*\{[^{}]*\}', html):
                rule = r.group(0).strip()
                if 'background' in rule or 'all:' in rule or 'appearance' in rule:
                    print('  RULE', rule[:400])
    break
# element-level resets
for r in re.finditer(r'[^{}]*\bbutton\b[^{}]*\{[^{}]*(?:background|all:)[^{}]*\}', html):
    print('RESET', r.group(0).strip()[:300])
print('links', re.findall(r'<link[^>]+stylesheet[^>]*>', html)[:6])
