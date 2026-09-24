#!/usr/bin/env python3
"""locate.py FILE KEY [KEY...] — print line numbers of lines containing any KEY (plain substring) in FILE.
Cheap navigation for code the Aleph index does not carry yet."""
import sys

path = sys.argv[1]
keys = sys.argv[2:]
lines = open(path).read().split('\n')
print('=====', path, len(lines), 'lines')
for i, l in enumerate(lines, 1):
    for k in keys:
        if k in l:
            print(f'{i:6d} [{k}] {l.strip()[:130]}')
            break
