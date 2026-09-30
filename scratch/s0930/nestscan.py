"""List nested rules (a `{` opened inside a style rule's declaration block) in a CSS/HTML file.
usage: python3 nestscan.py <file>"""
import sys

s = open(sys.argv[1], errors='replace').read()
i, n = 0, len(s)
stack = []  # 'style' | 'at'
prelude_start = 0
out = []
while i < n:
    c = s[i]
    if c == '\\':
        i += 2
        continue
    if c in '"\'':
        j = s.find(c, i + 1)
        i = n if j < 0 else j + 1
        continue
    if s.startswith('/*', i):
        j = s.find('*/', i + 2)
        i = n if j < 0 else j + 2
        continue
    if c == '{':
        pre = s[prelude_start:i].strip()
        kind = 'at' if pre.startswith('@') else 'style'
        if stack and stack[-1][0] == 'style':
            out.append((stack[-1][1], pre, s[i:i + 120]))
        stack.append((kind, pre))
        prelude_start = i + 1
    elif c == '}':
        if stack:
            stack.pop()
        prelude_start = i + 1
    elif c == ';':
        prelude_start = i + 1
    i += 1
print(len(out), 'nested rules')
for parent, pre, body in out:
    print(f'{parent[:70]!r}  >>  {pre[:60]!r}  {body[:100]!r}')
