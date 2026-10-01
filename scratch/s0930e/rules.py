"""Print every CSS rule in a saved page whose selector mentions any of the given class names,
with a little brace-aware context (the enclosing at-rule / parent rule prelude).
usage: python3 rules.py <html> <class>..."""
import re, sys
src = open(sys.argv[1], errors='replace').read()
names = sys.argv[2:]
for css in re.findall(r'<style[^>]*>(.*?)</style>', src, flags=re.S):
    stack = []  # preludes of open blocks
    i = 0
    start = 0
    n = len(css)
    while i < n:
        c = css[i]
        if c == '{':
            prelude = css[start:i].strip()
            stack.append(prelude)
            if any(re.search(r'\.' + re.escape(nm) + r'(?![\w-])', prelude) for nm in names):
                # find matching close
                depth, j = 1, i + 1
                while j < n and depth:
                    depth += css[j] == '{'
                    depth -= css[j] == '}'
                    j += 1
                ctx = ' >> '.join(p[:60] for p in stack[:-1])
                print(f'[{ctx}] {prelude} {css[i:j][:600]}')
            start = i + 1
        elif c == '}':
            if stack:
                stack.pop()
            start = i + 1
        elif c == ';':
            start = i + 1
        i += 1
