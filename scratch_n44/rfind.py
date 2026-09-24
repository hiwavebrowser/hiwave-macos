#!/usr/bin/env python3
"""rfind.py DIR SUBSTR [SUBSTR...] — file:line for every *.rs line under DIR containing any SUBSTR."""
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
keys = sys.argv[2:]
for p in sorted(root.rglob('*.rs')):
    if 'target' in p.parts:
        continue
    try:
        lines = p.read_text().split('\n')
    except Exception:
        continue
    for i, l in enumerate(lines, 1):
        for k in keys:
            if k in l:
                print(f'{p}:{i}: {l.strip()[:120]}')
                break
