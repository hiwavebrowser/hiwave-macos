#!/usr/bin/env python3
"""Bisect which html-rule declaration (with the * rule present) kills body padding."""
html = open('websuite/micro/bg-solid/index.html').read()
LINK = '<link rel="stylesheet" href="../../common/parity-reset.css">'

reset = open('websuite/common/parity-reset.css').read()
star = reset[reset.find('*, *::before'):reset.find('html {')].strip()
body_rule = 'body {\n  min-height: 100vh;\n}'

html_decls = {
    'font': 'font-family: sans-serif;',
    'size': 'font-size: 16px;',
    'lh': 'line-height: 1.5;',
    'color': 'color: #000;',
    'bg': 'background: #fff;',
    'smooth': '-webkit-font-smoothing: antialiased;\n  -moz-osx-font-smoothing: grayscale;',
}
for name, decl in html_decls.items():
    css = star + '\nhtml {\n  ' + decl + '\n}\n' + body_rule
    v = html.replace(LINK, '<style>\n' + css + '\n</style>')
    open('parity-tests/repro/bisect2-' + name + '.html', 'w').write(v)
print('generated', sorted(html_decls))
