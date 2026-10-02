"""Replace one placeholder in a file with another file's text. usage: sub.py <in> <out> <KEY> <text-file>"""
import sys
src, out, key, txt = sys.argv[1:5]
s = open(src).read()
assert key in s, key
open(out, 'w').write(s.replace(key, open(txt).read().rstrip()))
print('->', out)
