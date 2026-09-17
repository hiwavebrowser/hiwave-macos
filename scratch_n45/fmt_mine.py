#!/usr/bin/env python3
"""rustfmt --check on named files, but only report hunks inside the given line ranges.
usage: fmt_mine.py file:start-end[,start-end] ..."""
import re
import subprocess
import sys

files = []
ranges = {}
for arg in sys.argv[1:]:
    f, rs = arg.split(':')
    files.append(f)
    ranges[f] = [tuple(int(x) for x in r.split('-')) for r in rs.split(',')]

r = subprocess.run(['rustfmt', '--check', '--edition', '2021'] + files, capture_output=True, text=True)
out = r.stdout + r.stderr
hunks = re.split(r'(?=^Diff in )', out, flags=re.M)
mine = 0
for h in hunks:
    m = re.match(r'Diff in (\S+?):(\d+):', h)
    if not m:
        continue
    f, line = m.group(1), int(m.group(2))
    for key in ranges:
        if f.endswith(key):
            for a, b in ranges[key]:
                if a - 5 <= line <= b + 5:
                    print(h)
                    mine += 1
print('hunks in my ranges:', mine, '| total hunks:', sum(1 for h in hunks if h.startswith('Diff in')))
