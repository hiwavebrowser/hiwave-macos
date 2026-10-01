"""List the non-ASCII characters of a fixture with a little context.
usage: nonascii.py <file> [substring filter]"""
import collections, sys, unicodedata

s = open(sys.argv[1], encoding='utf-8').read()
seen = collections.OrderedDict()
for i, c in enumerate(s):
    if ord(c) > 0x7f:
        nxt = s[i + 1] if i + 1 < len(s) else ''
        key = (c, nxt == '️')
        if key not in seen:
            seen[key] = s[max(0, i - 30):i + 30].replace('\n', ' ')
for (c, vs), ctx in seen.items():
    print(f"U+{ord(c):04X} {'+VS16' if vs else '     '} {unicodedata.name(c, '?'):40} | {ctx}")
