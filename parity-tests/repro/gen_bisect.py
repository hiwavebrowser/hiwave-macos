#!/usr/bin/env python3
"""Generate inner bisect variants of the websuite parity reset for bg-solid."""
html = open('websuite/micro/bg-solid/index.html').read()
LINK = '<link rel="stylesheet" href="../../common/parity-reset.css">'

reset = open('websuite/common/parity-reset.css').read()
# split into the three rules by blank lines after the header comment
star_start = reset.find('*, *::before')
html_start = reset.find('html {')
body_start = reset.find('body {')
star = reset[star_start:html_start].strip()
html_rule = reset[html_start:body_start].strip()
body_rule = reset[body_start:].strip()

html_rule_simplefont = html_rule.replace(
    'font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;',
    'font-family: sans-serif;')

variants = {
    'a1': star + '\n' + html_rule,                              # no body rule
    'a2': star + '\n' + body_rule,                              # no html rule
    'a3': star + '\n' + html_rule_simplefont + '\n' + body_rule,  # simple font list
    'a4': html_rule + '\n' + body_rule,                         # no star rule
}
for name, css in variants.items():
    v = html.replace(LINK, '<style>\n' + css + '\n</style>')
    open('parity-tests/repro/bisect-' + name + '.html', 'w').write(v)
print('generated', sorted(variants))
